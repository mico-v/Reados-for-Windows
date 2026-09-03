//! Bounded lookup and caller-owned environment utilities.
//!
//! These commands deliberately expose only virtual state already carried by a
//! [`CommandInvocation`].  They never read the process environment, resolve a
//! host `PATH`, inspect an executable, or dispatch another process/command.

use crate::{
    failure_with_limits, success_with_limits, validate_command_name, Command, CommandInvocation,
    CommandLimits, CommandOutput, OutputBuilder, WorkspaceBackend, MAX_COMMAND_ARGUMENT_BYTES,
    MAX_INVOCATION_ENVIRONMENT_ENTRIES,
};
use std::collections::BTreeSet;

const ENV_USAGE: &[u8] = b"env: usage: env [-0] [-u NAME|--unset NAME] [NAME ...]\n";
const WHICH_USAGE: &[u8] = b"which: usage: which [-a] NAME [NAME ...]\n";
const TYPE_USAGE: &[u8] = b"type: usage: type [-at] NAME [NAME ...]\n";
const COMMAND_USAGE: &[u8] = b"command: usage: command [-vV] NAME [NAME ...]\n";
const ENV_HELP: &[u8] = b"Usage: env [-0] [-u NAME|--unset NAME] [NAME ...]\nList or query the caller-provided virtual environment.\nAssignments and command execution are unsupported.\n";
const WHICH_HELP: &[u8] = b"Usage: which [-a] NAME [NAME ...]\nList registered virtual command names; no host PATH is searched.\n";
const TYPE_HELP: &[u8] =
    b"Usage: type [-a] [-t] NAME [NAME ...]\nDescribe registered virtual command names.\n";
const COMMAND_HELP: &[u8] = b"Usage: command [-vV] NAME [NAME ...]\nLook up registered virtual command names without executing them.\n";

/// The bounded virtual environment command.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvCommand;

impl Command for EnvCommand {
    fn name(&self) -> &str {
        "env"
    }

    fn summary(&self) -> Option<&str> {
        Some("list or query the caller-provided virtual environment")
    }

    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let plan = match parse_env(invocation.args(), invocation.limits()) {
            Ok(plan) => plan,
            Err(output) => return output,
        };
        if let Some(help) = plan.help {
            return success_with_limits(help, invocation.limits());
        }

        let environment = invocation.environment();
        let mut names = BTreeSet::new();
        if plan.queries.is_empty() {
            if let Some(environment) = environment {
                names.extend(environment.keys().cloned());
            }
            for name in &plan.unsets {
                names.remove(name);
            }
        } else {
            names.extend(plan.queries.iter().cloned());
        }

        let separator = if plan.null_terminated { b'\0' } else { b'\n' };
        let mut output = OutputBuilder::new(invocation.limits());
        let mut missing = false;
        for name in names {
            let Some(value) = environment.and_then(|values| values.get(&name)) else {
                if !plan.queries.is_empty() {
                    missing = true;
                }
                continue;
            };
            if !plan.queries.is_empty() && plan.unsets.contains(&name) {
                missing = true;
                continue;
            }
            // The command's public query form intentionally retains NAME= so
            // listing and querying have the same byte-oriented record format.
            if !output.stdout(name.as_bytes())
                || !output.stdout(b"=")
                || !output.stdout(value.as_bytes())
                || !output.stdout(&[separator])
            {
                return output.finish(1);
            }
        }
        output.finish(i32::from(missing))
    }
}

/// Lookup registered virtual command names without producing a host path.
#[derive(Clone, Copy, Debug, Default)]
pub struct WhichCommand;

impl Command for WhichCommand {
    fn name(&self) -> &str {
        "which"
    }

    fn summary(&self) -> Option<&str> {
        Some("look up registered virtual command names")
    }

    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let plan = match parse_which(invocation.args(), invocation.limits()) {
            Ok(plan) => plan,
            Err(output) => return output,
        };
        if let Some(help) = plan.help {
            return success_with_limits(help, invocation.limits());
        }
        if plan.operands.is_empty() {
            return failure_with_limits(1, &[], invocation.limits());
        }

        let names = invocation.registry_names();
        let mut output = OutputBuilder::new(invocation.limits());
        let mut missing = false;
        for operand in plan.operands {
            if names.is_some_and(|registered| registered.contains(&operand)) {
                // A bare registry name is the only stable virtual lookup
                // result.  In particular, never manufacture `/bin/...` or a
                // host executable path.
                if !output.stdout(operand.as_bytes()) || !output.stdout(b"\n") {
                    return output.finish(1);
                }
            } else {
                missing = true;
            }
        }
        output.finish(i32::from(missing))
    }
}

/// Describe registered virtual command names.
#[derive(Clone, Copy, Debug, Default)]
pub struct TypeCommand;

impl Command for TypeCommand {
    fn name(&self) -> &str {
        "type"
    }

    fn summary(&self) -> Option<&str> {
        Some("describe registered virtual command names")
    }

    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let plan = match parse_type(invocation.args(), invocation.limits()) {
            Ok(plan) => plan,
            Err(output) => return output,
        };
        if let Some(help) = plan.help {
            return success_with_limits(help, invocation.limits());
        }
        if plan.operands.is_empty() {
            return success_with_limits(&[], invocation.limits());
        }

        let names = invocation.registry_names();
        let mut output = OutputBuilder::new(invocation.limits());
        let mut missing = false;
        for operand in plan.operands {
            if names.is_some_and(|registered| registered.contains(&operand)) {
                let line = if plan.type_only {
                    b"virtual\n".as_slice()
                } else {
                    // Keep the diagnostic/result independent of host shell
                    // functions, aliases, PATH entries, and executable paths.
                    let mut line = Vec::with_capacity(operand.len() + 22);
                    line.extend_from_slice(operand.as_bytes());
                    line.extend_from_slice(b" is a virtual command\n");
                    if !output.stdout(&line) {
                        return output.finish(1);
                    }
                    continue;
                };
                if !output.stdout(line) {
                    return output.finish(1);
                }
            } else {
                missing = true;
                if !plan.type_only {
                    let _ = output.stderr(b"type: command not found\n");
                }
            }
        }
        output.finish(i32::from(missing))
    }
}

/// Lookup-only `command`; execution and host lookup modes are intentionally
/// rejected so this command cannot become an escape hatch from the registry.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandCommand;

impl Command for CommandCommand {
    fn name(&self) -> &str {
        "command"
    }

    fn summary(&self) -> Option<&str> {
        Some("look up registered virtual command names without executing them")
    }

    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let plan = match parse_command(invocation.args(), invocation.limits()) {
            Ok(plan) => plan,
            Err(output) => return output,
        };
        if let Some(help) = plan.help {
            return success_with_limits(help, invocation.limits());
        }
        if !plan.lookup && !plan.describe {
            return failure_with_limits(
                2,
                b"command: execution is unsupported\n",
                invocation.limits(),
            );
        }
        if plan.operands.is_empty() {
            return success_with_limits(&[], invocation.limits());
        }

        let names = invocation.registry_names();
        let mut output = OutputBuilder::new(invocation.limits());
        let mut missing = false;
        for operand in plan.operands {
            if names.is_some_and(|registered| registered.contains(&operand)) {
                if plan.describe {
                    let mut line = Vec::with_capacity(operand.len() + 22);
                    line.extend_from_slice(operand.as_bytes());
                    line.extend_from_slice(b" is a virtual command\n");
                    if !output.stdout(&line) {
                        return output.finish(1);
                    }
                } else if !output.stdout(operand.as_bytes()) || !output.stdout(b"\n") {
                    return output.finish(1);
                }
            } else {
                missing = true;
                if plan.describe {
                    let _ = output.stderr(b"command: command not found\n");
                }
            }
        }
        output.finish(i32::from(missing))
    }
}

#[derive(Debug)]
struct EnvPlan {
    null_terminated: bool,
    unsets: BTreeSet<String>,
    queries: BTreeSet<String>,
    help: Option<&'static [u8]>,
}

fn parse_env(arguments: &[String], limits: CommandLimits) -> Result<EnvPlan, CommandOutput> {
    let mut plan = EnvPlan {
        null_terminated: false,
        unsets: BTreeSet::new(),
        queries: BTreeSet::new(),
        help: None,
    };
    let mut options = true;
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = &arguments[index];
        if options && argument == "--" {
            options = false;
            index += 1;
            continue;
        }
        if options && argument == "--help" {
            plan.help = Some(ENV_HELP);
            return Ok(plan);
        }
        if options && argument == "--version" {
            plan.help = Some(b"env (ReadOS virtual utilities) 1\n");
            return Ok(plan);
        }
        if options && (argument == "-0" || argument == "--null") {
            plan.null_terminated = true;
            index += 1;
            continue;
        }
        if options && argument == "-u" {
            let Some(name) = arguments.get(index + 1) else {
                return Err(env_usage_error(limits));
            };
            validate_environment_operand(name).map_err(|_| env_usage_error(limits))?;
            insert_bounded(&mut plan.unsets, name).map_err(|_| env_limit_error(limits))?;
            index += 2;
            continue;
        }
        if options && argument.starts_with("-u") && argument.len() > 2 {
            let name = &argument[2..];
            validate_environment_operand(name).map_err(|_| env_usage_error(limits))?;
            insert_bounded(&mut plan.unsets, name).map_err(|_| env_limit_error(limits))?;
            index += 1;
            continue;
        }
        if options && argument == "--unset" {
            let Some(name) = arguments.get(index + 1) else {
                return Err(env_usage_error(limits));
            };
            validate_environment_operand(name).map_err(|_| env_usage_error(limits))?;
            insert_bounded(&mut plan.unsets, name).map_err(|_| env_limit_error(limits))?;
            index += 2;
            continue;
        }
        if options && argument.starts_with("--unset=") {
            let name = &argument[8..];
            validate_environment_operand(name).map_err(|_| env_usage_error(limits))?;
            insert_bounded(&mut plan.unsets, name).map_err(|_| env_limit_error(limits))?;
            index += 1;
            continue;
        }
        if options && argument.starts_with('-') {
            return Err(env_usage_error(limits));
        }
        if argument.contains('=') || validate_environment_operand(argument).is_err() {
            return Err(env_usage_error(limits));
        }
        insert_bounded(&mut plan.queries, argument).map_err(|_| env_limit_error(limits))?;
        index += 1;
    }
    Ok(plan)
}

#[derive(Debug)]
struct WhichPlan {
    operands: Vec<String>,
    help: Option<&'static [u8]>,
}

fn parse_which(arguments: &[String], limits: CommandLimits) -> Result<WhichPlan, CommandOutput> {
    let mut operands = BTreeSet::new();
    let mut options = true;
    for argument in arguments {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument == "--help" {
            return Ok(WhichPlan {
                operands: Vec::new(),
                help: Some(WHICH_HELP),
            });
        }
        if options && argument.starts_with('-') && argument != "-" {
            if argument[1..].chars().all(|character| character == 'a') {
                continue;
            }
            return Err(which_usage_error(limits));
        }
        validate_lookup_operand(argument).map_err(|_| which_usage_error(limits))?;
        insert_bounded(&mut operands, argument).map_err(|_| which_usage_error(limits))?;
    }
    Ok(WhichPlan {
        operands: operands.into_iter().collect(),
        help: None,
    })
}

#[derive(Debug)]
struct TypePlan {
    operands: Vec<String>,
    type_only: bool,
    help: Option<&'static [u8]>,
}

fn parse_type(arguments: &[String], limits: CommandLimits) -> Result<TypePlan, CommandOutput> {
    let mut operands = BTreeSet::new();
    let mut type_only = false;
    let mut options = true;
    for argument in arguments {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument == "--help" {
            return Ok(TypePlan {
                operands: Vec::new(),
                type_only,
                help: Some(TYPE_HELP),
            });
        }
        if options && argument == "--all" {
            continue;
        }
        if options && argument == "--type" {
            type_only = true;
            continue;
        }
        if options && argument.starts_with('-') && argument != "-" {
            if argument[1..]
                .chars()
                .all(|character| character == 'a' || character == 't')
            {
                type_only |= argument[1..].contains('t');
                continue;
            }
            // `-p`, `-P`, and `-f` are intentionally not accepted: they
            // imply host executable/function/path lookup outside this pack.
            return Err(type_usage_error(limits));
        }
        validate_lookup_operand(argument).map_err(|_| type_usage_error(limits))?;
        insert_bounded(&mut operands, argument).map_err(|_| type_usage_error(limits))?;
    }
    Ok(TypePlan {
        operands: operands.into_iter().collect(),
        type_only,
        help: None,
    })
}

#[derive(Debug)]
struct CommandPlan {
    lookup: bool,
    describe: bool,
    operands: Vec<String>,
    help: Option<&'static [u8]>,
}

fn parse_command(
    arguments: &[String],
    limits: CommandLimits,
) -> Result<CommandPlan, CommandOutput> {
    let mut lookup = false;
    let mut describe = false;
    let mut operands = BTreeSet::new();
    let mut options = true;
    for argument in arguments {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument == "--help" {
            return Ok(CommandPlan {
                lookup,
                describe,
                operands: Vec::new(),
                help: Some(COMMAND_HELP),
            });
        }
        if options && argument.starts_with('-') && argument != "-" {
            if argument[1..]
                .chars()
                .all(|character| character == 'v' || character == 'V')
            {
                lookup |= argument[1..].contains('v');
                describe |= argument[1..].contains('V');
                continue;
            }
            // `-p` is the upstream default-PATH mode and is explicitly
            // rejected because this command has no host lookup capability.
            return Err(command_usage_error(limits));
        }
        validate_lookup_operand(argument).map_err(|_| command_usage_error(limits))?;
        insert_bounded(&mut operands, argument).map_err(|_| command_usage_error(limits))?;
    }
    Ok(CommandPlan {
        lookup,
        describe,
        operands: operands.into_iter().collect(),
        help: None,
    })
}

fn validate_environment_operand(value: &str) -> Result<(), ()> {
    if value.len() > MAX_COMMAND_ARGUMENT_BYTES || value.contains('=') {
        return Err(());
    }
    let mut characters = value.chars();
    match characters.next() {
        Some('_' | 'A'..='Z' | 'a'..='z') => {}
        _ => return Err(()),
    }
    if characters.all(|character| character == '_' || character.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(())
    }
}

fn validate_lookup_operand(value: &str) -> Result<(), ()> {
    if value.len() > MAX_COMMAND_ARGUMENT_BYTES
        || value.contains(['/', '\\', ':'])
        || value.chars().any(char::is_control)
    {
        return Err(());
    }
    validate_command_name(value).map_err(|_| ())
}

fn insert_bounded(set: &mut BTreeSet<String>, value: &str) -> Result<(), ()> {
    if set.len() >= MAX_INVOCATION_ENVIRONMENT_ENTRIES && !set.contains(value) {
        return Err(());
    }
    set.insert(value.to_owned());
    Ok(())
}

fn env_usage_error(limits: CommandLimits) -> CommandOutput {
    failure_with_limits(2, ENV_USAGE, limits)
}

fn env_limit_error(limits: CommandLimits) -> CommandOutput {
    failure_with_limits(2, b"env: input limit exceeded\n", limits)
}

fn which_usage_error(limits: CommandLimits) -> CommandOutput {
    failure_with_limits(2, WHICH_USAGE, limits)
}

fn type_usage_error(limits: CommandLimits) -> CommandOutput {
    failure_with_limits(2, TYPE_USAGE, limits)
}

fn command_usage_error(limits: CommandLimits) -> CommandOutput {
    failure_with_limits(2, COMMAND_USAGE, limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandLimits, Registry};
    use msp_backend::InMemoryWorkspace;

    fn invocation(args: &[&str]) -> CommandInvocation {
        CommandInvocation::from_cwd("/work", args.iter().copied()).unwrap()
    }

    fn environment_invocation(args: &[&str], values: &[(&str, &str)]) -> CommandInvocation {
        invocation(args)
            .with_environment(values.iter().map(|(name, value)| (*name, *value)))
            .unwrap()
    }

    #[test]
    fn env_lists_sorted_and_queries_without_process_environment() {
        let result = EnvCommand.run(
            &environment_invocation(&[], &[("ZED", "last"), ("ALPHA", "first")]),
            &InMemoryWorkspace::new(),
        );
        assert_eq!(result.stdout(), b"ALPHA=first\nZED=last\n");
        assert_eq!(result.exit_code(), 0);

        let result = EnvCommand.run(
            &environment_invocation(&["ZED", "MISSING"], &[("ZED", "last")]),
            &InMemoryWorkspace::new(),
        );
        assert_eq!(result.stdout(), b"ZED=last\n");
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn env_unset_is_local_and_assignments_are_rejected() {
        let result = EnvCommand.run(
            &environment_invocation(&["-u", "SECRET"], &[("A", "one"), ("SECRET", "no")]),
            &InMemoryWorkspace::new(),
        );
        assert_eq!(result.stdout(), b"A=one\n");
        assert_eq!(result.exit_code(), 0);

        let result = EnvCommand.run(
            &environment_invocation(&["SECRET=value"], &[("A", "one")]),
            &InMemoryWorkspace::new(),
        );
        assert_eq!(result.exit_code(), 2);
        assert_eq!(result.stderr(), ENV_USAGE);
        assert!(!result.stderr().windows(6).any(|window| window == b"SECRET"));
    }

    #[test]
    fn lookup_commands_use_only_explicit_registry_names_and_virtual_results() {
        let registry = Registry::default();
        let names = registry.names().map(str::to_owned).collect::<Vec<_>>();
        let backend = InMemoryWorkspace::new();

        let which = invocation(&["echo"])
            .with_registry_names(names.clone())
            .unwrap();
        let result = WhichCommand.run(&which, &backend);
        assert_eq!(result.stdout(), b"echo\n");

        let missing = invocation(&["definitely-missing"])
            .with_registry_names(names.clone())
            .unwrap();
        let result = WhichCommand.run(&missing, &backend);
        assert_eq!(result.exit_code(), 1);
        assert!(result.stdout().is_empty());
        assert!(result.stderr().is_empty());

        let typed = invocation(&["-t", "echo"])
            .with_registry_names(names.clone())
            .unwrap();
        assert_eq!(TypeCommand.run(&typed, &backend).stdout(), b"virtual\n");

        let described = invocation(&["-V", "echo"])
            .with_registry_names(names.clone())
            .unwrap();
        assert_eq!(
            CommandCommand.run(&described, &backend).stdout(),
            b"echo is a virtual command\n"
        );
    }

    #[test]
    fn lookup_rejects_path_and_execution_modes_with_fixed_diagnostics() {
        let registry = Registry::default();
        let names = registry.names().map(str::to_owned).collect::<Vec<_>>();
        let backend = InMemoryWorkspace::new();
        let path = invocation(&["C:/secret/tool"])
            .with_registry_names(names.clone())
            .unwrap();
        let result = WhichCommand.run(&path, &backend);
        assert_eq!(result.exit_code(), 2);
        assert_eq!(result.stderr(), WHICH_USAGE);
        assert!(!result.stderr().windows(6).any(|window| window == b"secret"));

        let execute = invocation(&["echo"]).with_registry_names(names).unwrap();
        let result = CommandCommand.run(&execute, &backend);
        assert_eq!(result.exit_code(), 2);
        assert_eq!(result.stderr(), b"command: execution is unsupported\n");
    }

    #[test]
    fn utility_output_respects_custom_limits() {
        let limits = CommandLimits {
            max_stdout_bytes: 2,
            ..CommandLimits::default()
        };
        let invocation = CommandInvocation::from_parts(
            msp_backend::VirtualPath::new("/work").unwrap(),
            ["echo"],
            Some(Vec::new()),
            limits,
        )
        .unwrap()
        .with_registry_names(["echo"])
        .unwrap();
        let result = WhichCommand.run(&invocation, &InMemoryWorkspace::new());
        assert!(result.output_limit_exceeded());
        assert!(result.stdout().len() <= 2);
    }
}

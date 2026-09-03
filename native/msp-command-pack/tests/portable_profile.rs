use msp_backend::{InMemoryWorkspace, VirtualPath};
use msp_command_pack::{
    CommandInvocation, CommandLimits, Registry, PORTABLE_MSP_V1_COMMANDS, PORTABLE_MSP_V1_PROFILE,
};
use serde::Deserialize;
use std::collections::BTreeMap;

const PROFILE_JSON: &str = include_str!("../profile/portable_msp_v1.json");
const FIXTURES_JSON: &str = include_str!("../profile/portable_msp_v1_fixtures.json");

#[derive(Debug, Deserialize)]
struct FixtureSet {
    profile: String,
    workspace: BTreeMap<String, String>,
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
struct FixtureCase {
    id: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    stdin: Option<String>,
    #[serde(default)]
    #[serde(rename = "stdinBytes")]
    stdin_bytes: Vec<u8>,
    expected: ExpectedResult,
}

#[derive(Debug, Deserialize)]
struct ExpectedResult {
    stdout: Option<String>,
    #[serde(rename = "stdoutBytes")]
    stdout_bytes: Option<Vec<u8>>,
    stderr: Option<String>,
    #[serde(rename = "stderrBytes")]
    stderr_bytes: Option<Vec<u8>>,
    #[serde(rename = "exitCode")]
    exit_code: i32,
}

#[test]
fn portable_profile_manifest_matches_the_frozen_registry() {
    let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
    assert_eq!(profile["profile"], PORTABLE_MSP_V1_PROFILE);
    let names = profile["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    let mut sorted_names = names;
    sorted_names.sort_unstable();
    assert_eq!(sorted_names, PORTABLE_MSP_V1_COMMANDS);

    let excluded = profile["excluded_from_profile"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(excluded, ["command", "env", "type", "which"]);
}

#[test]
fn shared_portable_profile_fixtures_have_stable_results_and_no_host_disclosure() {
    let fixtures: FixtureSet = serde_json::from_str(FIXTURES_JSON).unwrap();
    assert_eq!(fixtures.profile, PORTABLE_MSP_V1_PROFILE);

    let mut workspace = InMemoryWorkspace::new();
    for (path, contents) in fixtures.workspace {
        workspace.put_file(path.as_str(), contents.as_bytes()).unwrap();
    }

    let registry = Registry::with_portable_msp_v1().unwrap();
    for fixture in fixtures.cases {
        let stdin = if !fixture.stdin_bytes.is_empty() {
            Some(fixture.stdin_bytes)
        } else {
            fixture.stdin.map(String::into_bytes)
        };
        let invocation = CommandInvocation::from_parts(
            VirtualPath::new("/work").unwrap(),
            fixture.args,
            stdin,
            CommandLimits::default(),
        )
        .unwrap();
        let result = registry.execute(&fixture.command, &invocation, &workspace);

        let expected_stdout = fixture
            .expected
            .stdout_bytes
            .or_else(|| fixture.expected.stdout.map(String::into_bytes))
            .unwrap_or_default();
        let expected_stderr = fixture
            .expected
            .stderr_bytes
            .or_else(|| fixture.expected.stderr.map(String::into_bytes))
            .unwrap_or_default();

        assert_eq!(
            result.exit_code(),
            fixture.expected.exit_code,
            "fixture {}",
            fixture.id
        );
        assert_eq!(result.stdout(), expected_stdout, "fixture {}", fixture.id);
        assert_eq!(result.stderr(), expected_stderr, "fixture {}", fixture.id);

        let combined = [result.stdout(), result.stderr()].concat();
        let disclosed = String::from_utf8_lossy(&combined);
        assert!(
            !disclosed.contains("C:\\"),
            "fixture {} disclosed a Windows path",
            fixture.id
        );
        assert!(
            !disclosed.contains("/Users/"),
            "fixture {} disclosed a macOS path",
            fixture.id
        );
        assert!(
            !disclosed.contains("/home/"),
            "fixture {} disclosed a Linux host path",
            fixture.id
        );
        assert!(
            !disclosed.contains("PATH="),
            "fixture {} disclosed host environment",
            fixture.id
        );
    }
}

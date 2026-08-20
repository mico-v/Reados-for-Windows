using System.Runtime.CompilerServices;
using System.Text;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeBackedCommandTests
{
    [Fact]
    public void Constructor_accepts_canonical_read_only_commands_and_rejects_null_dependencies()
    {
        using var provider = Provider(new RecordingAdapter());

        Assert.Throws<ArgumentNullException>(() =>
            new MspNativeBackedCommand(null!, provider));
        Assert.Throws<ArgumentNullException>(() =>
            new MspNativeBackedCommand(new EchoCommand(), null!));
        _ = new MspNativeBackedCommand(new LsCommand(), provider);
        _ = new MspNativeBackedCommand(new CatCommand(), provider);
        Assert.Throws<ArgumentException>(() =>
            new MspNativeBackedCommand(new HelpCommand(new MspCommandRegistry()), provider));
    }

    [Fact]
    public void Proxy_preserves_managed_metadata_preview_and_summary()
    {
        var fallback = new MetadataCommand();
        using var provider = Provider(new RecordingAdapter());
        var command = new MspNativeBackedCommand(fallback, provider);
        var arguments = new[] { "fixture" };

        var metadata = command.GetMetadata(arguments);
        var preview = command.GetPreview(arguments);

        Assert.Equal(fallback.Name, command.Name);
        Assert.Equal(fallback.Summary, command.Summary);
        Assert.Same(fallback.Metadata, command.Metadata);
        Assert.Equal(MspCommandEffects.ExternalModel, metadata.Effects);
        Assert.Equal("managed preview", preview.Summary);
        Assert.Equal(1, fallback.MetadataCalls);
        Assert.Equal(1, fallback.PreviewCalls);
    }

    [Fact]
    public async Task Canonical_command_uses_rust_parse_and_execute_with_original_request_context()
    {
        const string rawCommand = "echo -e 'line\\n'";
        var adapter = new RecordingAdapter
        {
            ParseResult = SuccessfulParse(
                "echo",
                rawCommand,
                arguments: ["-e", "line\\n"]),
            ExecuteResult = NativeResult(
                rawCommand,
                "echo",
                stdout: Encoding.UTF8.GetBytes("line\n\n"),
                arguments: ["-e", "line\\n"])
        };
        using var provider = Provider(adapter);
        var command = new MspNativeBackedCommand(new EchoCommand(), provider);
        var context = Context(
            commandName: "echo",
            commandText: rawCommand,
            actor: "operator",
            sessionId: "session-9",
            workingDirectory: "/documents");

        var result = await command.ExecuteAsync(context, ["-e", "line\\n"]);

        Assert.Equal("line\n\n", result.Stdout);
        Assert.Empty(result.AuditRecords);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(1, adapter.ExecuteCalls);
        Assert.Equal(rawCommand, adapter.LastParseRequest!.CommandText);
        var request = adapter.LastExecuteRequest!;
        Assert.Equal(rawCommand, request.CommandText);
        Assert.Equal("/documents", request.WorkingDirectory);
        Assert.Equal("operator", request.Actor);
        Assert.Equal("session-9", request.SessionId);
        Assert.False(request.DryRun);
        Assert.Null(request.WorkspaceRoot);
        Assert.Empty(request.Environment);
    }

    [Theory]
    [InlineData("ECHO")]
    [InlineData("Echo")]
    public async Task Noncanonical_case_fails_closed_without_managed_execute_or_native_load(
        string commandName)
    {
        var factoryCalls = 0;
        using var provider = new LazyMspNativeAdapterProvider(() =>
        {
            factoryCalls++;
            throw new InvalidOperationException("native should not load");
        });
        var managedDefinition = new TrackingCommand("echo");
        var command = new MspNativeBackedCommand(managedDefinition, provider);
        var context = Context(commandName, commandName + " hello");

        var result = await command.ExecuteAsync(context, ["hello"]);

        Assert.Equal(2, result.ExitCode);
        Assert.Equal(
            "msp.native.route.managed_command_mismatch",
            Assert.Single(result.Diagnostics).Code);
        Assert.Equal(0, factoryCalls);
        Assert.False(provider.IsAdapterCreated);
        Assert.Equal(0, managedDefinition.ExecuteCalls);
    }

    [Fact]
    public async Task Unrelated_managed_command_name_fails_without_fallback_or_native_load()
    {
        using var provider = Provider(new RecordingAdapter());
        var command = new MspNativeBackedCommand(new EchoCommand(), provider);

        var result = await command.ExecuteAsync(
            Context("pwd", "pwd"),
            Array.Empty<string>());

        Assert.Equal(2, result.ExitCode);
        Assert.Equal(
            "msp.native.route.managed_command_mismatch",
            Assert.Single(result.Diagnostics).Code);
        Assert.False(provider.IsAdapterCreated);
    }

    [Fact]
    public async Task Parser_disagreement_fails_closed_without_execute_or_managed_fallback()
    {
        var adapter = new RecordingAdapter
        {
            ParseResult = FailedParse()
        };
        using var provider = Provider(adapter);
        var fallback = new TrackingCommand("echo");
        var command = new MspNativeBackedCommand(fallback, provider);

        var result = await command.ExecuteAsync(
            Context("echo", "echo 'unterminated"),
            ["unterminated"]);

        Assert.Equal(2, result.ExitCode);
        Assert.Equal(
            "msp.native.route.parser_disagreement",
            Assert.Single(result.Diagnostics).Code);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Equal(0, fallback.ExecuteCalls);
    }

    [Fact]
    public async Task Compound_or_redirected_form_fails_closed_before_execute()
    {
        var adapter = new RecordingAdapter
        {
            ParseResult = SuccessfulParse(
                "echo",
                "echo safe && pwd",
                pipelines: 2)
        };
        using var provider = Provider(adapter);
        var fallback = new TrackingCommand("echo");
        var command = new MspNativeBackedCommand(fallback, provider);

        var result = await command.ExecuteAsync(
            Context("echo", "echo safe && pwd"),
            ["safe", "&&", "pwd"]);

        Assert.Equal(2, result.ExitCode);
        Assert.Equal(
            "msp.native.route.unsupported_shell_form",
            Assert.Single(result.Diagnostics).Code);
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Equal(0, fallback.ExecuteCalls);
    }

    [Fact]
    public async Task Missing_native_library_is_safe_failure_and_never_falls_back()
    {
        const string secret = @"V:\private\msp_core.dll";
        using var provider = new LazyMspNativeAdapterProvider(() =>
            throw new InvalidOperationException(secret));
        var fallback = new TrackingCommand("pwd");
        var command = new MspNativeBackedCommand(fallback, provider);

        var result = await command.ExecuteAsync(
            Context("pwd", "pwd"),
            Array.Empty<string>());

        Assert.Equal(1, result.ExitCode);
        Assert.Equal(
            "msp.native.library_unavailable",
            Assert.Single(result.Diagnostics).Code);
        Assert.DoesNotContain(secret, result.Stderr);
        Assert.Equal(0, fallback.ExecuteCalls);
        Assert.Empty(result.AuditRecords);
    }

    [Fact]
    public async Task Dry_run_is_bounded_managed_projection_and_never_loads_native()
    {
        using var provider = new LazyMspNativeAdapterProvider(() =>
            throw new InvalidOperationException("native should not load"));
        var command = new MspNativeBackedCommand(new EchoCommand(), provider);

        var result = await command.ExecuteAsync(
            Context("echo", "echo hello", dryRun: true),
            ["hello"]);

        Assert.Equal("dry-run: echo hello", result.Stdout);
        Assert.False(provider.IsAdapterCreated);
    }

    [Fact]
    public async Task Cancellation_before_route_prevents_native_load()
    {
        using var provider = Provider(new RecordingAdapter());
        var command = new MspNativeBackedCommand(new EchoCommand(), provider);
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();

        await Assert.ThrowsAsync<OperationCanceledException>(async () =>
            await command.ExecuteAsync(
                Context("echo", "echo hello"),
                ["hello"],
                cancellation.Token));
        Assert.False(provider.IsAdapterCreated);
    }

    [Fact]
    public async Task Outer_runtime_keeps_policy_approval_and_single_audit_authority()
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => SuccessfulParse(
                "echo",
                request.CommandText,
                arguments: ["approved"]),
            ExecuteResultFactory = request => NativeResult(
                request.CommandText,
                "echo",
                stdout: Encoding.UTF8.GetBytes("approved\n"),
                arguments: ["approved"],
                actor: request.Actor,
                sessionId: request.SessionId,
                workingDirectory: request.WorkingDirectory)
        };
        using var provider = Provider(adapter);
        var approvalGrants = new MspApprovalGrantStore();
        var host = CreateHost(
            provider,
            new ApprovalRequiredPolicy(approvalGrants),
            out var auditSink);
        var requestFactory = new MspCommandRequestFactory("session-1", "agent-1");

        var pending = await host.ExecuteAsync(requestFactory.Create("echo approved"));

        Assert.Equal(126, pending.ExitCode);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, Assert.Single(pending.AuditRecords).Decision);
        Assert.Equal(0, adapter.ParseCalls);
        Assert.Equal(0, adapter.ExecuteCalls);

        var approvedExecutor = new MspApprovedCommandExecutor(host, approvalGrants, requestFactory);
        var approved = await approvedExecutor.ExecuteApprovedAsync("echo approved", "operator");

        Assert.Equal("approved\n", approved.Stdout);
        var audit = Assert.Single(approved.AuditRecords);
        Assert.Equal(MspPolicyDecision.Allow, audit.Decision);
        Assert.Equal("operator", audit.Actor);
        Assert.Equal("session-1", audit.SessionId);
        Assert.Equal("echo", audit.CommandName);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(1, adapter.ExecuteCalls);
        Assert.Equal(2, auditSink.Records.Count);
        Assert.Equal(audit, auditSink.Records[1]);
    }

    [Fact]
    public async Task Outer_runtime_routes_explicit_empty_argument_when_both_parsers_preserve_it()
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => SuccessfulParse(
                "echo",
                request.CommandText,
                arguments: [""]),
            ExecuteResultFactory = request => NativeResult(
                request.CommandText,
                "echo",
                stdout: Encoding.UTF8.GetBytes("\n"),
                arguments: [""],
                actor: request.Actor,
                sessionId: request.SessionId,
                workingDirectory: request.WorkingDirectory)
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy(), out var auditSink);

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "echo ''",
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.Equal("\n", result.Stdout);
        Assert.Single(result.AuditRecords);
        Assert.Single(auditSink.Records);
        Assert.Equal(result.AuditRecords[0], auditSink.Records[0]);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(1, adapter.ExecuteCalls);
    }

    [Fact]
    public async Task Outer_runtime_rejects_execute_evidence_that_drifted_after_preflight()
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => SuccessfulParse(
                "echo",
                request.CommandText,
                arguments: ["safe"]),
            ExecuteResultFactory = request => NativeResult(
                request.CommandText,
                "echo",
                stdout: Encoding.UTF8.GetBytes("forged output\n"),
                arguments: ["forged"],
                actor: request.Actor,
                sessionId: request.SessionId,
                workingDirectory: request.WorkingDirectory)
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy());

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "echo safe",
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.Equal(1, result.ExitCode);
        Assert.Empty(result.Stdout);
        Assert.Equal(
            "msp.native.route.execution_mismatch",
            Assert.Single(result.Diagnostics).Code);
        Assert.Single(result.AuditRecords);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(1, adapter.ExecuteCalls);
    }

    [Theory]
    [InlineData("echo alpha\fbeta", "alpha\fbeta")]
    [InlineData("echo alpha\u2003beta", "alpha\u2003beta")]
    public async Task Outer_runtime_rejects_managed_native_whitespace_tokenization_disagreement(
        string commandText,
        string nativeArgument)
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => SuccessfulParse(
                "echo",
                request.CommandText,
                arguments: [nativeArgument])
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy());

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = commandText,
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.Equal(2, result.ExitCode);
        Assert.Equal(
            "msp.native.route.parser_disagreement",
            Assert.Single(result.Diagnostics).Code);
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Single(result.AuditRecords);
    }

    [Theory]
    [InlineData("echo x | pwd", "msp.parse", 0)]
    [InlineData("echo x |& pwd", "msp.parse", 0)]
    [InlineData("echo x > out", "msp.parse", 0)]
    [InlineData("echo x 2>&1", "msp.parse", 0)]
    [InlineData("echo x; pwd", "msp.parse", 0)]
    [InlineData("echo x || pwd", "msp.parse", 0)]
    [InlineData("X=1 echo x", "msp.command_not_found", 0)]
    [InlineData("! echo x", "msp.command_not_found", 0)]
    [InlineData("echo x && pwd", "msp.native.route.unsupported_shell_form", 1)]
    [InlineData("echo x\npwd", "msp.native.route.unsupported_shell_form", 1)]
    [InlineData("echo x & pwd", "msp.native.route.unsupported_shell_form", 1)]
    public async Task Outer_runtime_blocks_hostile_raw_forms_before_native_execute(
        string commandText,
        string expectedDiagnosticCode,
        int expectedParseCalls)
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => request.CommandText switch
            {
                "echo x && pwd" => SuccessfulParse(
                    "echo",
                    request.CommandText,
                    pipelines: 2,
                    arguments: ["x"]),
                "echo x\npwd" => SuccessfulParse(
                    "echo",
                    request.CommandText,
                    arguments: ["x", "pwd"]),
                "echo x & pwd" => SuccessfulParse(
                    "echo",
                    request.CommandText,
                    arguments: ["x", "&", "pwd"]),
                _ => throw new InvalidOperationException(
                    "The managed parser or registry should have rejected this form first.")
            }
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy(), out var auditSink);

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = commandText,
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.NotEqual(0, result.ExitCode);
        Assert.Equal(expectedDiagnosticCode, Assert.Single(result.Diagnostics).Code);
        Assert.Equal(expectedParseCalls, adapter.ParseCalls);
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Equal(expectedParseCalls > 0, provider.IsAdapterCreated);
        var audit = Assert.Single(result.AuditRecords);
        Assert.Single(auditSink.Records);
        Assert.Equal(audit, auditSink.Records[0]);
        Assert.Equal(
            expectedParseCalls == 0
                ? MspPolicyDecision.NotEvaluated
                : MspPolicyDecision.Allow,
            audit.Decision);
    }

    [Fact]
    public async Task Outer_runtime_routes_quoted_operator_literals_by_ast_not_raw_regex()
    {
        const string literal = "a|b; > && ! X=1";
        const string commandText = "echo -n 'a|b; > && ! X=1'";
        var adapter = new RecordingAdapter
        {
            ParseResult = SuccessfulParse(
                "echo",
                commandText,
                arguments: ["-n", literal],
                argumentsAreQuoted: true),
            ExecuteResult = NativeResult(
                commandText,
                "echo",
                stdout: Encoding.UTF8.GetBytes(literal),
                arguments: ["-n", literal])
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy());

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = commandText,
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Equal(literal, result.Stdout);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(1, adapter.ExecuteCalls);
        Assert.Single(result.AuditRecords);
    }

    [Fact]
    public async Task Outer_runtime_owns_dry_run_and_streaming_terminal_events()
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => SuccessfulParse(
                "echo",
                request.CommandText,
                arguments: ["streamed"]),
            ExecuteResultFactory = request => NativeResult(
                request.CommandText,
                "echo",
                stdout: Encoding.UTF8.GetBytes("streamed\n"),
                arguments: ["streamed"],
                actor: request.Actor,
                sessionId: request.SessionId,
                workingDirectory: request.WorkingDirectory)
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy());

        var dryRun = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "echo skipped",
            Actor = "tester",
            SessionId = "session-1",
            DryRun = true
        });

        Assert.Equal("dry-run: echo skipped", dryRun.Stdout);
        Assert.Single(dryRun.AuditRecords);
        Assert.Equal(0, adapter.ParseCalls);
        Assert.Equal(0, adapter.ExecuteCalls);

        var events = new List<MspCommandEvent>();
        await foreach (var commandEvent in host.ExecuteStreamingAsync(new MspCommandRequest
        {
            CommandText = "echo streamed",
            Actor = "tester",
            SessionId = "session-1"
        }))
        {
            events.Add(commandEvent);
        }

        Assert.Single(events, item => item.Kind == MspCommandEventKind.Started);
        Assert.Single(events, item => item.Kind == MspCommandEventKind.PolicyDecision);
        var terminal = Assert.Single(events, item => item.Kind == MspCommandEventKind.Completed);
        Assert.NotNull(terminal.Result);
        Assert.Single(terminal.Result.AuditRecords);
        Assert.Equal("streamed\n", terminal.Result.Stdout);
        Assert.Equal(1, adapter.ParseCalls);
        Assert.Equal(1, adapter.ExecuteCalls);
    }

    [Theory]
    [InlineData("provider")]
    [InlineData("parse")]
    [InlineData("execute")]
    public async Task Cancellation_during_native_route_produces_one_managed_canceled_terminal(
        string stage)
    {
        using var cancellation = new CancellationTokenSource();
        RecordingAdapter? adapter = null;
        IMspNativeAdapterProvider provider;
        if (stage == "provider")
        {
            provider = new LazyMspNativeAdapterProvider(() =>
            {
                cancellation.Cancel();
                cancellation.Token.ThrowIfCancellationRequested();
                return null!;
            });
        }
        else
        {
            adapter = new RecordingAdapter
            {
                ParseResultFactory = request =>
                {
                    if (stage == "parse")
                    {
                        cancellation.Cancel();
                        cancellation.Token.ThrowIfCancellationRequested();
                    }

                    return SuccessfulParse(
                        "echo",
                        request.CommandText,
                        arguments: ["value"]);
                },
                ExecuteResultFactory = request =>
                {
                    var result = NativeResult(
                        request.CommandText,
                        "echo",
                        stdout: Encoding.UTF8.GetBytes("native completed\n"),
                        arguments: ["value"],
                        actor: request.Actor,
                        sessionId: request.SessionId,
                        workingDirectory: request.WorkingDirectory);
                    if (stage == "execute")
                    {
                        cancellation.Cancel();
                    }

                    return result;
                }
            };
            provider = Provider(adapter);
        }

        using (provider)
        {
            var host = CreateHost(provider, new AllowAllMspPolicy(), out var auditSink);
            var events = new List<MspCommandEvent>();

            await foreach (var commandEvent in host.ExecuteStreamingAsync(new MspCommandRequest
            {
                CommandText = "echo value",
                Actor = "tester",
                SessionId = "session-1"
            }, cancellation.Token))
            {
                events.Add(commandEvent);
            }

            var terminal = Assert.Single(events, item =>
                item.Kind is MspCommandEventKind.Canceled or MspCommandEventKind.Completed);
            Assert.Equal(MspCommandEventKind.Canceled, terminal.Kind);
            Assert.Equal(130, terminal.ExitCode);
            Assert.NotNull(terminal.Result);
            var audit = Assert.Single(terminal.Result.AuditRecords);
            Assert.Equal(130, audit.ExitCode);
            Assert.Equal(MspPolicyDecision.Allow, audit.Decision);
            Assert.Equal("msp.canceled", Assert.Single(audit.Diagnostics).Code);
            Assert.Empty(terminal.Result.Stdout);
            Assert.Equal("Command execution was canceled.", terminal.Result.Stderr);
            Assert.Single(auditSink.Records);
            Assert.Equal(audit, auditSink.Records[0]);
        }

        if (adapter is not null)
        {
            Assert.Equal(stage == "parse" ? 0 : 1, adapter.ExecuteCalls);
        }
    }

    [Fact]
    public async Task Pwd_and_echo_share_one_lazy_adapter_instance()
    {
        var factoryCalls = 0;
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request =>
            {
                var parts = request.CommandText.Split(' ', StringSplitOptions.RemoveEmptyEntries);
                return SuccessfulParse(
                    parts[0],
                    request.CommandText,
                    arguments: parts.Skip(1).ToArray());
            },
            ExecuteResultFactory = request => NativeResult(
                request.CommandText,
                request.CommandText.Split(' ', 2)[0],
                stdout: Encoding.UTF8.GetBytes("ok\n"),
                arguments: request.CommandText == "pwd" ? [] : ["value"],
                actor: request.Actor,
                sessionId: request.SessionId,
                workingDirectory: request.WorkingDirectory)
        };
        using var provider = new LazyMspNativeAdapterProvider(() =>
        {
            factoryCalls++;
            return adapter;
        });
        var host = CreateHost(provider, new AllowAllMspPolicy());

        var pwd = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "pwd",
            Actor = "tester",
            SessionId = "session-1"
        });
        var echo = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "echo value",
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.True(pwd.Succeeded, pwd.Stderr);
        Assert.True(echo.Succeeded, echo.Stderr);
        Assert.Equal(1, factoryCalls);
        Assert.Equal(2, adapter.ParseCalls);
        Assert.Equal(2, adapter.ExecuteCalls);
    }

    [Fact]
    public async Task Explicit_pwd_echo_registry_keeps_unadopted_commands_managed()
    {
        var adapter = new RecordingAdapter
        {
            ParseResultFactory = request => SuccessfulParse(
                "echo",
                request.CommandText,
                arguments: []),
            ExecuteResultFactory = request => NativeResult(request.CommandText, "echo")
        };
        using var provider = Provider(adapter);
        var host = CreateHost(provider, new AllowAllMspPolicy());

        var uppercase = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "ECHO legacy",
            Actor = "tester",
            SessionId = "session-1"
        });
        var ls = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "ls /",
            Actor = "tester",
            SessionId = "session-1"
        });

        Assert.Equal(2, uppercase.ExitCode);
        Assert.Equal(
            "msp.native.route.managed_command_mismatch",
            Assert.Single(uppercase.Diagnostics).Code);
        Assert.True(ls.Succeeded, ls.Stderr);
        Assert.Equal(0, adapter.ParseCalls);
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Single(uppercase.AuditRecords);
        Assert.Single(ls.AuditRecords);
    }

    private static MspRuntimeCommandHost CreateHost(
        IMspNativeAdapterProvider provider,
        IMspPolicy policy)
    {
        return CreateHost(provider, policy, out _);
    }

    private static MspRuntimeCommandHost CreateHost(
        IMspNativeAdapterProvider provider,
        IMspPolicy policy,
        out InMemoryMspAuditSink auditSink)
    {
        var registry = MspRuntime.CreateDefaultRegistry();
        Assert.True(registry.TryGet("pwd", out var pwd));
        Assert.True(registry.TryGet("echo", out var echo));
        registry.Register(new MspNativeBackedCommand(pwd, provider));
        registry.Register(new MspNativeBackedCommand(echo, provider));
        auditSink = new InMemoryMspAuditSink();
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            policy,
            auditSink);
        return new MspRuntimeCommandHost(new MspRuntime(context));
    }

    private static LazyMspNativeAdapterProvider Provider(RecordingAdapter adapter)
    {
        return new LazyMspNativeAdapterProvider(() => adapter);
    }

    private static MspCommandContext Context(
        string commandName,
        string commandText,
        string actor = "tester",
        string sessionId = "session-1",
        string workingDirectory = "/",
        bool dryRun = false)
    {
        return new MspCommandContext(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink(),
            workingDirectory,
            invocation: new MspCommandInvocation
            {
                Actor = actor,
                SessionId = sessionId,
                CommandName = commandName,
                CommandText = commandText,
                DryRun = dryRun
            });
    }

    private static MspNativeShellParseResult SuccessfulParse(
        string commandName,
        string rawInput,
        int pipelines = 1,
        IReadOnlyList<string>? arguments = null,
        bool argumentsAreQuoted = false)
    {
        arguments ??= Array.Empty<string>();
        var parsedPipelines = Enumerable.Range(0, pipelines)
            .Select(index => new MspNativeParsedCommandPipeline
            {
                LeadingOperator = index == 0 ? null : MspNativeParsedListOperator.And,
                IsNegated = false,
                Commands = [SimpleCommand(
                    commandName,
                    rawInput,
                    arguments,
                    argumentsAreQuoted)],
                PipeOperators = []
            })
            .ToArray();
        return new MspNativeShellParseResult
        {
            ContractVersion = MspNativeContract.Version,
            Succeeded = true,
            Script = new MspNativeParsedShellScript
            {
                RawInput = rawInput,
                Pipelines = parsedPipelines
            }
        };
    }

    private static MspNativeShellParseResult FailedParse()
    {
        return new MspNativeShellParseResult
        {
            ContractVersion = MspNativeContract.Version,
            Succeeded = false,
            Error = new MspNativeShellParseError
            {
                Kind = MspNativeShellParseErrorKind.Syntax,
                ExitCode = 2,
                Message = "native parse failed"
            }
        };
    }

    private static MspNativeParsedCommandLine SimpleCommand(
        string commandName,
        string rawInput,
        IReadOnlyList<string> arguments,
        bool argumentsAreQuoted = false)
    {
        return new MspNativeParsedCommandLine
        {
            CommandName = commandName,
            Arguments = arguments.ToArray(),
            Assignments = [],
            Redirections = [],
            IsAssignmentOnly = false,
            RawInput = rawInput,
            CommandNameWord = new MspNativeParsedWord
            {
                Parts = [new MspNativeParsedWordPart
                {
                    Text = commandName,
                    IsExpandable = true,
                    IsQuoted = false
                }],
                HasExplicitEmptyQuotedFragment = false
            },
            ArgumentWords = arguments.Select(argument => new MspNativeParsedWord
            {
                Parts = [new MspNativeParsedWordPart
                {
                    Text = argument,
                    IsExpandable = true,
                    IsQuoted = argumentsAreQuoted
                }],
                HasExplicitEmptyQuotedFragment = argument.Length == 0
            }).ToArray()
        };
    }

    private static MspNativeCommandResult NativeResult(
        string commandText,
        string commandName,
        byte[]? stdout = null,
        byte[]? stderr = null,
        int exitCode = 0,
        IReadOnlyList<string>? arguments = null,
        string actor = "tester",
        string sessionId = "session-1",
        string workingDirectory = "/")
    {
        return new MspNativeCommandResult(
            exitCode,
            stdout ?? Array.Empty<byte>(),
            stderr ?? Array.Empty<byte>(),
            null,
            [new MspNativeAuditRecord
            {
                RunId = "native-run-1",
                CommandLine = commandText,
                CommandName = commandName,
                Arguments = arguments ?? [],
                ExitCode = exitCode,
                StartedAtUnixMs = 1,
                EndedAtUnixMs = 2,
                Actor = actor,
                SessionId = sessionId,
                WorkingDirectory = workingDirectory,
                PolicyDecision = new MspNativePolicyDecision
                {
                    Kind = MspNativePolicyDecisionKind.Allow
                },
                Diagnostics = []
            }],
            []);
    }

    private sealed class RecordingAdapter : IMspNativeAdapter
    {
        public MspNativeShellParseResult ParseResult { get; init; } =
            SuccessfulParse("echo", "echo");

        public Func<MspNativeShellParseRequest, MspNativeShellParseResult>? ParseResultFactory { get; init; }

        public MspNativeCommandResult ExecuteResult { get; init; } =
            NativeResult("echo", "echo");

        public Func<MspNativeCommandRequest, MspNativeCommandResult>? ExecuteResultFactory { get; init; }

        public int ParseCalls { get; private set; }

        public int ExecuteCalls { get; private set; }

        public MspNativeShellParseRequest? LastParseRequest { get; private set; }

        public MspNativeCommandRequest? LastExecuteRequest { get; private set; }

        public bool IsDisposed { get; private set; }

        public MspNativeCommandResult Execute(MspNativeCommandRequest request)
        {
            ExecuteCalls++;
            LastExecuteRequest = request;
            return ExecuteResultFactory?.Invoke(request) ?? ExecuteResult;
        }

        public MspNativeShellParseResult Parse(MspNativeShellParseRequest request)
        {
            ParseCalls++;
            LastParseRequest = request;
            return ParseResultFactory?.Invoke(request) ?? ParseResult;
        }

        public MspNativeWorkspacePathResult NormalizeWorkspacePath(
            MspNativeWorkspacePathRequest request)
        {
            throw new NotSupportedException();
        }

        public void Dispose()
        {
            IsDisposed = true;
        }
    }

    private sealed class TrackingCommand(string name) : IMspCommand
    {
        public int ExecuteCalls { get; private set; }

        public string Name { get; } = name;

        public string Summary => "tracking fallback";

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            ExecuteCalls++;
            return ValueTask.FromResult(MspCommandResult.Success("managed"));
        }
    }

    private sealed class MetadataCommand : IMspCommand
    {
        public int MetadataCalls { get; private set; }

        public int PreviewCalls { get; private set; }

        public string Name => "echo";

        public string Summary => "managed summary";

        public MspCommandMetadata Metadata { get; } = MspCommandMetadata.Create(
            "echo",
            "managed summary",
            "echo [text]");

        public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
        {
            MetadataCalls++;
            return MspCommandMetadata.Create(
                Name,
                Summary,
                effects: MspCommandEffects.ExternalModel);
        }

        public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
        {
            PreviewCalls++;
            return MspCommandPreview.Create("managed preview");
        }

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspCommandResult.Success("managed"));
        }
    }

    private sealed class ApprovalRequiredPolicy(IMspApprovalGrantStore grants) : IMspPolicy
    {
        public ValueTask<MspPolicyDecision> AuthorizeAsync(
            MspPolicyRequest request,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(grants.TryConsumeApproval(request)
                ? MspPolicyDecision.Allow
                : MspPolicyDecision.RequireConfirmation);
        }
    }
}

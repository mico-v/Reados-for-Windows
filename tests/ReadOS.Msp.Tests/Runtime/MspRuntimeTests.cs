using ReadOS.Msp.Audit;
using ReadOS.Msp.Models;
using ReadOS.Msp.Parsing;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Tests.Runtime;

public sealed class MspRuntimeTests
{
    [Fact]
    public void Parser_preserves_quoted_arguments()
    {
        var parsed = MspCommandLineParser.Parse("echo \"hello world\" '/docs/a b.txt'");

        Assert.Equal("echo", parsed.Name);
        Assert.Equal(new[] { "hello world", "/docs/a b.txt" }, parsed.Arguments);
    }

    [Fact]
    public async Task Runtime_executes_basic_workspace_commands()
    {
        var workspace = new InMemoryMspWorkspace();
        await workspace.WriteTextAsync("/notes/intro.txt", "hello MSP");
        var runtime = MspRuntime.CreateDefault(workspace);

        var list = await runtime.ExecuteAsync(new MspCommandRequest { CommandText = "ls /notes" });
        var cat = await runtime.ExecuteAsync(new MspCommandRequest { CommandText = "cat /notes/intro.txt" });

        Assert.True(list.Succeeded, list.Stderr);
        Assert.Contains("intro.txt", list.Stdout);
        Assert.True(cat.Succeeded, cat.Stderr);
        Assert.Equal("hello MSP" + Environment.NewLine, cat.Stdout);
    }

    [Fact]
    public async Task Runtime_records_audit_for_each_command()
    {
        var runtime = MspRuntime.CreateDefault();

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "test-agent",
            CommandText = "echo audit"
        });

        var record = Assert.Single(result.AuditRecords);
        Assert.Equal("test-agent", record.Actor);
        Assert.Equal("echo", record.CommandName);
        Assert.Equal(0, record.ExitCode);
    }

    [Fact]
    public async Task Runtime_returns_recovery_diagnostics_for_unknown_commands()
    {
        var runtime = MspRuntime.CreateDefault();

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "missing-command"
        });

        Assert.False(result.Succeeded);
        Assert.Equal(127, result.ExitCode);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal(MspDiagnosticSeverity.Error, diagnostic.Severity);
        Assert.Equal("msp.command_not_found", diagnostic.Code);
        Assert.Equal("missing-command", diagnostic.Target);
        Assert.Contains("Run help", diagnostic.RecoveryHint);
    }

    [Fact]
    public async Task Artifact_command_writes_lists_and_shows_workspace_artifacts()
    {
        var workspace = new InMemoryMspWorkspace();
        var runtime = MspRuntime.CreateDefault(workspace);

        var write = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "artifact-agent",
            SessionId = "session-42",
            CommandText = "artifact write /artifacts/summary.md \"hello artifacts\""
        });
        var list = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact list /artifacts"
        });
        var show = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact show /artifacts/summary.md"
        });

        Assert.True(write.Succeeded, write.Stderr);
        var artifact = Assert.Single(write.Artifacts);
        Assert.Equal("/artifacts/summary.md", artifact.Path);
        Assert.Equal("text/markdown", artifact.MediaType);
        Assert.Equal("artifact-agent", artifact.Actor);
        Assert.Equal("session-42", artifact.SessionId);
        Assert.Equal("artifact write /artifacts/summary.md \"hello artifacts\"", artifact.SourceCommand);
        Assert.Equal("contentLength: 15", artifact.Preview);
        Assert.True(list.Succeeded, list.Stderr);
        Assert.Contains("/artifacts/summary.md", list.Stdout);
        Assert.Contains("/artifacts/summary.md.manifest.json", list.Stdout);
        Assert.True(show.Succeeded, show.Stderr);
        Assert.Equal("hello artifacts" + Environment.NewLine, show.Stdout);

        var manifest = await workspace.TryReadTextAsync("/artifacts/summary.md.manifest.json");
        Assert.NotNull(manifest);
        Assert.Contains("\"actor\": \"artifact-agent\"", manifest);
        Assert.Contains("\"sessionId\": \"session-42\"", manifest);
        Assert.Contains("\"sourceCommand\": \"artifact write /artifacts/summary.md", manifest);
    }

    [Fact]
    public async Task Workflow_summary_writes_artifact_with_session_transcript_provenance()
    {
        var workspace = new InMemoryMspWorkspace();
        await workspace.WriteTextAsync("/sessions/workflow-1.json", """
            {
              "id": "workflow-1",
              "title": "Evidence workflow",
              "actor": "workflow-agent",
              "startedAt": "2026-06-30T01:00:00Z",
              "updatedAt": "2026-06-30T01:05:00Z",
              "lastCommandText": "missing-command",
              "lastDecision": "Allow",
              "lastExitCode": 127,
              "lastDiagnosticsSummary": "error msp.command_not_found: missing-command",
              "lastRecoveryHint": "Run help.",
              "commandCount": 2,
              "approvalCount": 1,
              "failureCount": 1,
              "transcriptIds": [ "ok-1", "fail-1" ],
              "artifactPaths": [ "/artifacts/excerpts/alpha.md" ]
            }
            """);
        await workspace.WriteTextAsync("/transcripts/ok-1.json", """
            {
              "id": "ok-1",
              "actor": "workflow-agent",
              "sessionId": "workflow-1",
              "commandText": "pdf text current 1 2 --artifact /artifacts/excerpts/alpha.md",
              "startedAt": "2026-06-30T01:01:00Z",
              "completedAt": "2026-06-30T01:02:00Z",
              "exitCode": 0,
              "stdout": "artifact\t/artifacts/excerpts/alpha.md\t42\n",
              "decision": "Allow",
              "effects": "ReadWorkspace, WriteWorkspace, CreateArtifact",
              "artifactsSummary": "/artifacts/excerpts/alpha.md"
            }
            """);
        await workspace.WriteTextAsync("/transcripts/fail-1.json", """
            {
              "id": "fail-1",
              "actor": "workflow-agent",
              "sessionId": "workflow-1",
              "commandText": "missing-command",
              "startedAt": "2026-06-30T01:03:00Z",
              "completedAt": "2026-06-30T01:04:00Z",
              "exitCode": 127,
              "stderr": "Command not found: missing-command",
              "decision": "Allow",
              "effects": "None",
              "diagnosticsSummary": "error msp.command_not_found: Command not found: missing-command",
              "recoveryHint": "Run help to list available MSP commands."
            }
            """);
        var runtime = MspRuntime.CreateDefault(workspace);

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "workflow-agent",
            SessionId = "workflow-1",
            CommandText = "workflow summary current --artifact /artifacts/workflows/workflow-1.md"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Contains("# MSP Workflow Summary", result.Stdout);
        Assert.Contains("- failures: 1", result.Stdout);
        Assert.Contains("msp.command_not_found", result.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/workflow-1.md", result.Stdout);

        var artifact = Assert.Single(result.Artifacts);
        Assert.Equal("/artifacts/workflows/workflow-1.md", artifact.Path);
        Assert.Equal("text/markdown", artifact.MediaType);
        Assert.Equal("workflow-agent", artifact.Actor);
        Assert.Equal("workflow-1", artifact.SessionId);
        Assert.Equal("workflow summary current --artifact /artifacts/workflows/workflow-1.md", artifact.SourceCommand);
        Assert.Equal(new[]
        {
            "/sessions/workflow-1.json",
            "/transcripts/ok-1.json",
            "/transcripts/fail-1.json"
        }, artifact.SourcePaths);

        var content = await workspace.TryReadTextAsync("/artifacts/workflows/workflow-1.md");
        Assert.NotNull(content);
        Assert.Contains("Evidence workflow", content);
        Assert.Contains("missing-command", content);

        var manifest = await workspace.TryReadTextAsync("/artifacts/workflows/workflow-1.md.manifest.json");
        Assert.NotNull(manifest);
        Assert.Contains("\"sourcePaths\": [", manifest);
        Assert.Contains("\"/sessions/workflow-1.json\"", manifest);
        Assert.Contains("\"/transcripts/fail-1.json\"", manifest);
    }

    [Fact]
    public async Task Runtime_passes_command_metadata_to_policy_and_audit()
    {
        var policy = new CapturingPolicy();
        var registry = new MspCommandRegistry().Register(new MutatingTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            policy,
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "metadata-test",
            SessionId = "metadata-session",
            CommandText = "mutate"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.NotNull(policy.LastRequest);
        Assert.Equal(MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, policy.LastRequest!.Effects);
        Assert.True(policy.LastRequest.RequiresConfirmation);
        Assert.Equal("msp.test.write", Assert.Single(policy.LastRequest.Capabilities));
        Assert.Empty(policy.LastRequest.Environment);
        Assert.Equal("Mutate test preview.", policy.LastRequest.Preview.Summary);
        Assert.Equal("/target", Assert.Single(policy.LastRequest.Preview.Targets));
        Assert.Equal("metadata-session", policy.LastRequest.SessionId);

        var record = Assert.Single(result.AuditRecords);
        Assert.Equal("metadata-session", record.SessionId);
        Assert.Equal(policy.LastRequest.Effects, record.Effects);
        Assert.Equal(policy.LastRequest.Preview, record.Preview);
    }

    [Fact]
    public async Task Runtime_passes_environment_to_policy()
    {
        var policy = new CapturingPolicy();
        var registry = new MspCommandRegistry().Register(new MutatingTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            policy,
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);

        await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "mutate",
            Environment = new Dictionary<string, string>
            {
                ["approval"] = "token"
            }
        });

        Assert.NotNull(policy.LastRequest);
        Assert.Equal("token", policy.LastRequest!.Environment["approval"]);
    }

    [Fact]
    public async Task Effect_policy_requires_confirmation_for_mutating_commands_and_returns_audit()
    {
        var registry = new MspCommandRegistry().Register(new MutatingTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new EffectBasedMspPolicy(),
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "policy-test",
            CommandText = "mutate"
        });

        Assert.False(result.Succeeded);
        Assert.Equal(126, result.ExitCode);
        Assert.Contains(nameof(MspPolicyDecision.RequireConfirmation), result.Stderr);

        var record = Assert.Single(result.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, record.Decision);
        Assert.Equal(MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, record.Effects);
        Assert.Equal(126, record.ExitCode);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("msp.policy.require_confirmation", diagnostic.Code);
        Assert.Contains("Approve", diagnostic.RecoveryHint);
        Assert.Equal(result.Diagnostics, record.Diagnostics);
    }

    [Fact]
    public async Task Effect_policy_allows_mutating_dry_runs()
    {
        var registry = new MspCommandRegistry().Register(new MutatingTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new EffectBasedMspPolicy(),
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "mutate",
            DryRun = true
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Contains("dry-run: mutate", result.Stdout);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(result.AuditRecords).Decision);
    }

    [Fact]
    public async Task Effect_policy_uses_argument_specific_command_metadata()
    {
        var workspace = new InMemoryMspWorkspace();
        await workspace.WriteTextAsync("/artifacts/summary.md", "hello");
        var registry = MspRuntime.CreateDefaultRegistry();
        var context = new MspCommandContext(
            workspace,
            registry,
            new EffectBasedMspPolicy(),
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);

        var list = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact list /artifacts"
        });
        var write = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact write /artifacts/new.md \"new artifact\""
        });

        Assert.True(list.Succeeded, list.Stderr);
        Assert.Equal(MspCommandEffects.ReadWorkspace, Assert.Single(list.AuditRecords).Effects);
        Assert.False(write.Succeeded);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, Assert.Single(write.AuditRecords).Decision);
        Assert.Equal(MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, Assert.Single(write.AuditRecords).Effects);
    }

    [Fact]
    public async Task Runtime_streams_started_policy_progress_and_completed_events()
    {
        var registry = new MspCommandRegistry().Register(new ProgressTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in runtime.ExecuteStreamingAsync(new MspCommandRequest
        {
            Actor = "stream-agent",
            SessionId = "stream-session",
            CommandText = "progress"
        }))
        {
            events.Add(commandEvent);
        }

        Assert.Equal(MspCommandEventKind.Started, events[0].Kind);
        Assert.Equal("progress", events[0].CommandName);
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.PolicyDecision &&
            item.Decision == MspPolicyDecision.Allow);
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.CommandName == "progress" &&
            item.Percent == 40 &&
            item.Message == "halfway");
        var completed = events[^1];
        Assert.Equal(MspCommandEventKind.Completed, completed.Kind);
        Assert.Equal(0, completed.ExitCode);
        Assert.NotNull(completed.Result);
        Assert.Equal("done", completed.Result.Stdout);
        Assert.All(events, item => Assert.Equal("stream-session", item.SessionId));
    }

    [Fact]
    public async Task Runtime_streams_completed_result_for_parse_failures()
    {
        var runtime = MspRuntime.CreateDefault();
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in runtime.ExecuteStreamingAsync(new MspCommandRequest
        {
            CommandText = "echo \"unterminated"
        }))
        {
            events.Add(commandEvent);
        }

        var completed = Assert.Single(events);
        Assert.Equal(MspCommandEventKind.Completed, completed.Kind);
        Assert.Equal(2, completed.ExitCode);
        Assert.NotNull(completed.Result);
        var diagnostic = Assert.Single(completed.Result!.Diagnostics);
        Assert.Equal("msp.parse", diagnostic.Code);
        Assert.Contains("quoting", diagnostic.RecoveryHint);
    }

    [Fact]
    public async Task Runtime_streams_canceled_event_when_command_is_canceled()
    {
        var registry = new MspCommandRegistry().Register(new CancelingTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in runtime.ExecuteStreamingAsync(new MspCommandRequest
        {
            CommandText = "cancel-me"
        }))
        {
            events.Add(commandEvent);
        }

        Assert.Contains(events, item => item.Kind == MspCommandEventKind.Progress);
        Assert.Equal(MspCommandEventKind.Canceled, events[^1].Kind);
        Assert.Equal(130, events[^1].Result?.ExitCode);
    }

    private sealed class CapturingPolicy : IMspPolicy
    {
        public MspPolicyRequest? LastRequest { get; private set; }

        public ValueTask<MspPolicyDecision> AuthorizeAsync(
            MspPolicyRequest request,
            CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            return ValueTask.FromResult(MspPolicyDecision.Allow);
        }
    }

    private sealed class MutatingTestCommand : IMspCommand
    {
        public string Name => "mutate";

        public string Summary => "Mutate test workspace.";

        public MspCommandMetadata Metadata => MspCommandMetadata.Create(
            Name,
            Summary,
            "mutate",
            MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
            new[] { "msp.test.write" });

        public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
        {
            return MspCommandPreview.Create(
                "Mutate test preview.",
                new[] { "/target" },
                new[] { "detail: test" });
        }

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspCommandResult.Success());
        }
    }

    private sealed class ProgressTestCommand : IMspCommand
    {
        public string Name => "progress";

        public string Summary => "Emit test progress.";

        public async ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            await context.ReportProgressAsync("halfway", 40, cancellationToken);
            return MspCommandResult.Success("done");
        }
    }

    private sealed class CancelingTestCommand : IMspCommand
    {
        public string Name => "cancel-me";

        public string Summary => "Cancel test command.";

        public async ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            await context.ReportProgressAsync("before cancel", 10, cancellationToken);
            throw new OperationCanceledException(cancellationToken);
        }
    }
}

using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspExecSessionHostFacadeTests
{
    [Fact]
    public void Constructor_rejects_null_adapter_provider()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new MspExecSessionHostFacade(null!));
    }

    [Fact]
    public async Task ExecCommandAsync_formats_codex_terminal_header_with_output()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 41,
            exitCode: 0,
            terminalText: "hello session\n",
            wallTimeSeconds: 0.0031));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.ExecCommandAsync("echo hello session");

        Assert.Equal(41UL, read.SessionId);
        Assert.Equal(0, read.ExitCode);
        Assert.False(read.Running);
        Assert.Equal(
            "Wall time: 0.0031 seconds\nProcess exited with code 0\nOutput:\nhello session\n",
            read.TerminalText);
        Assert.Null(read.Error);

        Assert.NotNull(adapter.LastRequest);
        Assert.Equal("echo hello session", adapter.LastRequest!.CommandText);
        Assert.Equal(0UL, adapter.LastRequest.SessionId);
    }

    [Fact]
    public async Task ExecCommandAsync_passes_working_directory_yield_and_token_bounds()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        await facade.ExecCommandAsync(
            "echo hi",
            workingDirectory: "/documents",
            yieldTimeMs: 500,
            maxOutputTokens: 128);

        Assert.NotNull(adapter.LastRequest);
        Assert.Equal("/documents", adapter.LastRequest!.WorkingDirectory);
        Assert.Equal(500, adapter.LastRequest.YieldTimeMs);
        Assert.Equal(128, adapter.LastRequest.MaxOutputTokens);
    }

    [Fact]
    public async Task ExecCommandAsync_formats_running_header_with_session_id()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 92,
            running: true,
            exitCode: null,
            terminalText: null,
            wallTimeSeconds: 0.25));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.ExecCommandAsync("long-running");

        Assert.True(read.Running);
        Assert.Null(read.ExitCode);
        Assert.Equal(
            "Wall time: 0.2500 seconds\nProcess running with session ID 92\nOutput:\n",
            read.TerminalText);
    }

    [Fact]
    public async Task WriteStdinAsync_empty_poll_formats_retained_terminal_text()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 42,
            exitCode: 0,
            terminalText: "retained output\n"));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.WriteStdinAsync(42);

        Assert.Equal(42UL, read.SessionId);
        Assert.Equal(0, read.ExitCode);
        Assert.Contains("Process exited with code 0", read.TerminalText);
        Assert.Contains("retained output", read.TerminalText);

        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(42UL, adapter.LastRequest!.SessionId);
        Assert.Null(adapter.LastRequest.Chars);
    }

    [Fact]
    public async Task WriteStdinAsync_nonempty_passes_chars_and_formats_inactive_session_envelope()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 42,
            exitCode: 1,
            terminalText: "write_stdin failed: inactive session 42\n"));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.WriteStdinAsync(42, "data\n");

        Assert.Equal(1, read.ExitCode);
        Assert.Contains("Process exited with code 1", read.TerminalText);
        Assert.Contains("write_stdin failed: inactive session 42", read.TerminalText);

        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(42UL, adapter.LastRequest!.SessionId);
        Assert.Equal("data\n", adapter.LastRequest.Chars);
    }

    [Fact]
    public async Task Ok_false_maps_to_error_envelope_without_surfacing_raw_json()
    {
        var adapter = new RecordingExecSessionAdapter(_ => new MspNativeExecSessionResult
        {
            ContractVersion = MspNativeContract.Version,
            Ok = false,
            SessionId = 9,
            Error = new MspNativeExecSessionError
            {
                Code = "msp.exec_session.inactive",
                Message = "no such session"
            }
        });
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.WriteStdinAsync(9);

        Assert.Equal("no such session", read.Error);
        Assert.Null(read.ExitCode);
        Assert.Equal(string.Empty, read.TerminalText);
        Assert.DoesNotContain("msp.exec_session.inactive", read.TerminalText);
        Assert.DoesNotContain("{", read.TerminalText);
    }

    [Fact]
    public async Task ExecCommandAsync_canceled_before_call_throws_without_invoking_adapter()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));
        using var cts = new CancellationTokenSource();
        cts.Cancel();

        await Assert.ThrowsAsync<OperationCanceledException>(() =>
            facade.ExecCommandAsync("echo hi", ct: cts.Token));

        Assert.Null(adapter.LastRequest);
    }

    [Fact]
    public async Task ExecCommandAsync_canceled_after_native_call_throws_operation_canceled()
    {
        using var cts = new CancellationTokenSource();
        var adapter = new RecordingExecSessionAdapter(_ =>
        {
            // The synchronous native call completes, then the facade observes
            // the cancellation that arrived in flight (no preemption).
            cts.Cancel();
            return ExecResult();
        });
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        await Assert.ThrowsAsync<OperationCanceledException>(() =>
            facade.ExecCommandAsync("echo hi", ct: cts.Token));

        Assert.NotNull(adapter.LastRequest);
    }

    [Fact]
    public async Task ExecCommandAsync_process_mode_maps_program_arguments_workspace_root_and_bounds()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 41,
            exitCode: 0,
            terminalText: "process output\n",
            wallTimeSeconds: 0.0031));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));
        var environment = new Dictionary<string, string>
        {
            ["MSP_TEST_VALUE"] = "secret-value",
            ["MSP_TEST_MODE"] = "process"
        };

        var read = await facade.ExecCommandAsync(
            MspExecSessionMode.Process,
            "C:\\tools\\runner.exe",
            new[] { "--flag", "value" },
            "C:\\workspace",
            yieldTimeMs: 500,
            maxOutputTokens: 128,
            environment: environment);

        Assert.Equal(41UL, read.SessionId);
        Assert.Equal(0, read.ExitCode);
        Assert.Contains("process output", read.TerminalText);

        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Process, adapter.LastRequest!.Mode);
        Assert.Equal("C:\\tools\\runner.exe", adapter.LastRequest.Program);
        Assert.Equal(new[] { "--flag", "value" }, adapter.LastRequest.Arguments);
        Assert.Equal("C:\\workspace", adapter.LastRequest.WorkspaceRoot);
        Assert.Equal(environment, adapter.LastRequest.Environment);
        Assert.DoesNotContain("secret-value", read.TerminalText);
        Assert.Equal(500, adapter.LastRequest.YieldTimeMs);
        Assert.Equal(128, adapter.LastRequest.MaxOutputTokens);
        Assert.Equal(0UL, adapter.LastRequest.SessionId);
    }

    [Fact]
    public async Task ExecCommandAsync_shell_mode_omits_process_fields()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.ExecCommandAsync(
            MspExecSessionMode.Shell,
            "echo hi",
            new[] { "--ignored" },
            "C:\\ignored",
            environment: new Dictionary<string, string>
            {
                ["SHELL_SECRET"] = "must-not-forward"
            });

        Assert.Equal(1UL, read.SessionId);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Shell, adapter.LastRequest!.Mode);
        Assert.Equal("echo hi", adapter.LastRequest.CommandText);
        Assert.Null(adapter.LastRequest.Program);
        Assert.Null(adapter.LastRequest.Arguments);
        Assert.Null(adapter.LastRequest.WorkspaceRoot);
        Assert.Null(adapter.LastRequest.Environment);
    }

    [Fact]
    public async Task ReadSession_returns_raw_record_without_codex_header()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 42,
            exitCode: 0,
            terminalText: "raw retained output\n",
            wallTimeSeconds: 0.25));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var read = await facade.ReadSession(42);

        Assert.Equal(42UL, read.SessionId);
        Assert.Equal(0, read.ExitCode);
        Assert.Equal("raw retained output\n", read.TerminalText);
        Assert.DoesNotContain("Wall time:", read.TerminalText);
        Assert.DoesNotContain("Output:", read.TerminalText);
    }

    [Fact]
    public async Task ExecAsync_typed_request_maps_shell_fields_to_native_and_returns_plain_response()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 17,
            terminalText: "typed output\n",
            wallTimeSeconds: 0.125));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var response = await facade.ExecAsync(new MspExecSessionRequest
        {
            Command = "echo typed",
            WorkingDirectory = "/documents",
            YieldTimeMs = 250,
            MaxOutputTokens = 64
        });

        Assert.Equal(17UL, response.SessionId);
        Assert.Equal(0, response.ExitCode);
        Assert.Contains("Wall time: 0.1250 seconds", response.TerminalText);
        Assert.Contains("typed output", response.TerminalText);
        Assert.Null(response.Error);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Shell, adapter.LastRequest!.Mode);
        Assert.Equal("echo typed", adapter.LastRequest.CommandText);
        Assert.Equal("/documents", adapter.LastRequest.WorkingDirectory);
        Assert.Equal(250, adapter.LastRequest.YieldTimeMs);
        Assert.Equal(64, adapter.LastRequest.MaxOutputTokens);
        Assert.Null(adapter.LastRequest.Environment);
    }

    [Fact]
    public async Task ExecCommandAsync_typed_process_request_forwards_environment_without_leaking_it_or_root()
    {
        const string workspaceRoot = @"C:\private\reados-workspace";
        const string secret = "typed-secret";
        var environment = new Dictionary<string, string>
        {
            ["MSP_TYPED_VALUE"] = secret
        };
        var adapter = new RecordingExecSessionAdapter(_ => new MspNativeExecSessionResult
        {
            ContractVersion = MspNativeContract.Version,
            Ok = false,
            Error = new MspNativeExecSessionError
            {
                Code = "msp.process.spawn",
                Message = "failed at " + workspaceRoot
            }
        });
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var response = await facade.ExecCommandAsync(new MspExecSessionRequest
        {
            Mode = MspExecSessionMode.Process,
            Program = @"C:\tools\runner.exe",
            Arguments = new[] { "--typed" },
            WorkspaceRoot = workspaceRoot,
            Environment = environment
        });

        Assert.Equal("The native exec session failed.", response.Error);
        Assert.DoesNotContain(workspaceRoot, response.Error, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain(secret, response.TerminalText);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Process, adapter.LastRequest!.Mode);
        Assert.Equal(@"C:\tools\runner.exe", adapter.LastRequest.Program);
        Assert.Equal(new[] { "--typed" }, adapter.LastRequest.Arguments);
        Assert.Equal(workspaceRoot, adapter.LastRequest.WorkspaceRoot);
        Assert.Equal(environment, adapter.LastRequest.Environment);
    }

    [Fact]
    public async Task Typed_stdin_poll_and_continuation_preserve_empty_vs_nonempty_chars()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 23,
            terminalText: "retained\n"));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var poll = await facade.PollStdinAsync(new MspExecSessionStdinPollRequest
        {
            SessionId = 23,
            YieldTimeMs = 500
        });

        Assert.Equal(23UL, poll.SessionId);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(23UL, adapter.LastRequest!.SessionId);
        Assert.Null(adapter.LastRequest.Chars);
        Assert.Equal(500, adapter.LastRequest.YieldTimeMs);

        var continuation = await facade.ContinueStdinAsync(
            new MspExecSessionStdinContinuationRequest
            {
                SessionId = 23,
                Chars = "next\n",
                MaxOutputTokens = 32
            });

        Assert.Equal(23UL, continuation.SessionId);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(23UL, adapter.LastRequest!.SessionId);
        Assert.Equal("next\n", adapter.LastRequest.Chars);
        Assert.Equal(32, adapter.LastRequest.MaxOutputTokens);
    }

    [Fact]
    public async Task Typed_continuation_rejects_oversized_chars_before_adapter_call()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var exception = await Assert.ThrowsAsync<ArgumentOutOfRangeException>(() =>
            facade.ContinueStdinAsync(new MspExecSessionStdinContinuationRequest
            {
                SessionId = 23,
                Chars = new string('x', MspNativeExecSessionLimits.MaximumWriteStdinChars + 1)
            }));

        Assert.DoesNotContain("xxx", exception.ToString());
        Assert.Null(adapter.LastRequest);
    }

    [Fact]
    public async Task Typed_process_environment_count_is_bounded_before_adapter_call()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));
        var environment = Enumerable.Range(
                0,
                MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries + 1)
            .ToDictionary(index => $"MSP_TYPED_{index}", index => index.ToString());

        var exception = await Assert.ThrowsAsync<ArgumentOutOfRangeException>(() =>
            facade.ExecAsync(new MspExecSessionRequest
            {
                Mode = MspExecSessionMode.Process,
                Program = @"C:\tools\runner.exe",
                WorkspaceRoot = @"C:\workspace",
                Environment = environment
            }));

        Assert.Contains(
            MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries.ToString(),
            exception.Message);
        Assert.Null(adapter.LastRequest);
    }

    [Fact]
    public async Task Typed_exec_checks_cancellation_before_adapter_call()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));
        using var cts = new CancellationTokenSource();
        cts.Cancel();

        await Assert.ThrowsAsync<OperationCanceledException>(() =>
            facade.ExecAsync(new MspExecSessionRequest
            {
                CommandText = "echo canceled"
            }, cts.Token));

        Assert.Null(adapter.LastRequest);
    }

    [Fact]
    public async Task ExecJsonAsync_maps_strict_model_arguments_to_typed_shell_request()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 31,
            terminalText: "json output\n",
            wallTimeSeconds: 0.125));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var response = await facade.ExecJsonAsync(
            "{\"cmd\":\"echo json\",\"workdir\":\"/documents\",\"yield_time_ms\":250,\"max_output_tokens\":64}");

        Assert.Equal(31UL, response.SessionId);
        Assert.Contains("json output", response.TerminalText);
        Assert.Null(response.Error);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Shell, adapter.LastRequest!.Mode);
        Assert.Equal("echo json", adapter.LastRequest.CommandText);
        Assert.Equal("/documents", adapter.LastRequest.WorkingDirectory);
        Assert.Equal(250, adapter.LastRequest.YieldTimeMs);
        Assert.Equal(64, adapter.LastRequest.MaxOutputTokens);
        Assert.Null(adapter.LastRequest.Program);
    }

    [Fact]
    public async Task DispatchJsonAsync_routes_empty_and_nonempty_stdin_to_poll_and_continuation()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult(
            sessionId: 42,
            terminalText: "retained output\n"));
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var poll = await facade.DispatchJsonAsync(
            "write_stdin",
            "{\"session_id\":42,\"yield_time_ms\":500}");

        Assert.Equal(42UL, poll.SessionId);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(42UL, adapter.LastRequest!.SessionId);
        Assert.Null(adapter.LastRequest.Chars);
        Assert.Equal(500, adapter.LastRequest.YieldTimeMs);

        var continuation = await facade.WriteStdinJsonAsync(
            "{\"session_id\":42,\"chars\":\"next\\n\",\"max_output_tokens\":32}");

        Assert.Equal(42UL, continuation.SessionId);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal("next\n", adapter.LastRequest!.Chars);
        Assert.Equal(32, adapter.LastRequest.MaxOutputTokens);
    }

    [Fact]
    public async Task ExecJsonAsync_process_request_forwards_bounds_and_withholds_host_error_details()
    {
        const string workspaceRoot = @"C:\private\json-workspace";
        const string secret = "json-secret";
        var adapter = new RecordingExecSessionAdapter(_ => new MspNativeExecSessionResult
        {
            ContractVersion = MspNativeContract.Version,
            Ok = false,
            Error = new MspNativeExecSessionError
            {
                Code = "msp.process.spawn",
                Message = "failed at " + workspaceRoot
            }
        });
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var response = await facade.ExecJsonAsync(
            "{\"mode\":\"process\",\"program\":\"C:\\\\tools\\\\runner.exe\",\"arguments\":[\"--json\"],\"workspaceRoot\":\"C:\\\\private\\\\json-workspace\",\"environment\":{\"MSP_JSON_VALUE\":\"json-secret\"}}");

        Assert.Equal("The native exec session failed.", response.Error);
        Assert.DoesNotContain(workspaceRoot, response.Error, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain(secret, response.TerminalText);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Process, adapter.LastRequest!.Mode);
        Assert.Equal(@"C:\tools\runner.exe", adapter.LastRequest.Program);
        Assert.Equal(new[] { "--json" }, adapter.LastRequest.Arguments);
        Assert.Equal(workspaceRoot, adapter.LastRequest.WorkspaceRoot);
        Assert.Equal(secret, adapter.LastRequest.Environment!["MSP_JSON_VALUE"]);
    }

    [Fact]
    public async Task Json_arguments_reject_unknown_duplicate_and_out_of_range_fields_before_adapter_call()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var unknown = await Assert.ThrowsAsync<ArgumentException>(() =>
            facade.ExecJsonAsync("{\"cmd\":\"echo\",\"unknown\":\"C:\\\\private\"}"));
        Assert.Contains("unsupported", unknown.Message, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("C:\\private", unknown.ToString(), StringComparison.OrdinalIgnoreCase);
        Assert.Null(adapter.LastRequest);

        var duplicate = await Assert.ThrowsAsync<ArgumentException>(() =>
            facade.ExecJsonAsync("{\"cmd\":\"echo one\",\"command\":\"echo two\"}"));
        Assert.Contains("duplicate", duplicate.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Null(adapter.LastRequest);

        var negative = await Assert.ThrowsAsync<ArgumentException>(() =>
            facade.WriteStdinJsonAsync("{\"session_id\":42,\"max_output_tokens\":-1}"));
        Assert.Contains("non-negative", negative.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Null(adapter.LastRequest);
    }

    [Fact]
    public async Task WriteStdinJsonAsync_rejects_zero_session_and_oversized_chars_before_adapter_call()
    {
        var adapter = new RecordingExecSessionAdapter(_ => ExecResult());
        var facade = new MspExecSessionHostFacade(new FakeAdapterProvider(adapter));

        var zero = await Assert.ThrowsAsync<ArgumentException>(() =>
            facade.WriteStdinJsonAsync("{\"session_id\":0}"));
        Assert.Contains("non-zero", zero.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Null(adapter.LastRequest);

        var oversizedChars = new string('x', MspNativeExecSessionLimits.MaximumWriteStdinChars + 1);
        var oversizedJson = "{\"session_id\":42,\"chars\":\"" + oversizedChars + "\"}";
        var oversized = await Assert.ThrowsAsync<ArgumentException>(() =>
            facade.WriteStdinJsonAsync(oversizedJson));
        Assert.Contains("maximum", oversized.Message, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("xxx", oversized.ToString());
        Assert.Null(adapter.LastRequest);
    }

    private static MspNativeExecSessionResult ExecResult(
        ulong sessionId = 1,
        bool running = false,
        string? terminalText = null,
        int? exitCode = 0,
        double wallTimeSeconds = 0.001)
    {
        return new MspNativeExecSessionResult
        {
            ContractVersion = MspNativeContract.Version,
            Ok = true,
            SessionId = sessionId,
            Running = running,
            TerminalText = terminalText,
            ExitCode = exitCode,
            WallTimeSeconds = wallTimeSeconds,
            Truncated = false,
            Error = null
        };
    }

    private sealed class FakeAdapterProvider(IMspNativeAdapter adapter) : IMspNativeAdapterProvider
    {
        public bool IsAdapterCreated => true;

        public IMspNativeAdapter GetRequiredAdapter() => adapter;

        public void Dispose()
        {
        }
    }

    private sealed class RecordingExecSessionAdapter(
        Func<MspNativeExecSessionRequest, MspNativeExecSessionResult> handler) :
        IMspNativeAdapter
    {
        public MspNativeExecSessionRequest? LastRequest { get; private set; }

        public MspNativeCommandResult Execute(MspNativeCommandRequest request)
        {
            throw new NotSupportedException();
        }

        public MspNativeShellParseResult Parse(MspNativeShellParseRequest request)
        {
            throw new NotSupportedException();
        }

        public MspNativeWorkspacePathResult NormalizeWorkspacePath(MspNativeWorkspacePathRequest request)
        {
            throw new NotSupportedException();
        }

        public MspNativeExecSessionResult ExecSession(
            MspNativeExecSessionRequest request,
            CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            return handler(request);
        }

        public void Dispose()
        {
        }
    }
}

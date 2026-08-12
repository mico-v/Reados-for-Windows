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

        var read = await facade.ExecCommandAsync(
            MspExecSessionMode.Process,
            "C:\\tools\\runner.exe",
            new[] { "--flag", "value" },
            "C:\\workspace",
            yieldTimeMs: 500,
            maxOutputTokens: 128);

        Assert.Equal(41UL, read.SessionId);
        Assert.Equal(0, read.ExitCode);
        Assert.Contains("process output", read.TerminalText);

        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Process, adapter.LastRequest!.Mode);
        Assert.Equal("C:\\tools\\runner.exe", adapter.LastRequest.Program);
        Assert.Equal(new[] { "--flag", "value" }, adapter.LastRequest.Arguments);
        Assert.Equal("C:\\workspace", adapter.LastRequest.WorkspaceRoot);
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
            "C:\\ignored");

        Assert.Equal(1UL, read.SessionId);
        Assert.NotNull(adapter.LastRequest);
        Assert.Equal(MspExecSessionMode.Shell, adapter.LastRequest!.Mode);
        Assert.Equal("echo hi", adapter.LastRequest.CommandText);
        Assert.Null(adapter.LastRequest.Program);
        Assert.Null(adapter.LastRequest.Arguments);
        Assert.Null(adapter.LastRequest.WorkspaceRoot);
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

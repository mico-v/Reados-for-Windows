using System.Text;
using System.Text.Json;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeExecSessionTests
{
    [Fact]
    public void Exec_maps_to_operation_five_and_serializes_exec_request()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(
                sessionId: 41,
                exitCode: 0,
                terminalText: "hello session\n"),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.ExecSession(new MspNativeExecSessionRequest
        {
            CommandText = "echo hello session",
            WorkingDirectory = "/documents",
            Actor = "测试者",
            DryRun = false,
            YieldTimeMs = 500,
            MaxOutputTokens = 128
        });

        Assert.Equal(MspNativeOperation.ExecSession, transport.LastOperation);
        Assert.True(result.Ok);
        Assert.Equal(41UL, result.SessionId);
        Assert.Equal(0, result.ExitCode);
        Assert.False(result.Running);
        Assert.Equal("hello session\n", result.TerminalText);
        Assert.Null(result.Error);

        using var request = JsonDocument.Parse(transport.LastRequestJson);
        Assert.Equal(MspNativeContract.Version, request.RootElement.GetProperty("contractVersion").GetString());
        Assert.Equal("exec", request.RootElement.GetProperty("kind").GetString());
        Assert.Equal("echo hello session", request.RootElement.GetProperty("commandText").GetString());
        Assert.Equal(0UL, request.RootElement.GetProperty("sessionId").GetUInt64());
        Assert.Equal("/documents", request.RootElement.GetProperty("workingDirectory").GetString());
        Assert.Equal("测试者", request.RootElement.GetProperty("actor").GetString());
        Assert.False(request.RootElement.GetProperty("dryRun").GetBoolean());
        Assert.Equal(500, request.RootElement.GetProperty("yieldTimeMs").GetInt32());
        Assert.Equal(128, request.RootElement.GetProperty("maxOutputTokens").GetInt32());
        Assert.False(request.RootElement.TryGetProperty("chars", out _));
        Assert.False(request.RootElement.TryGetProperty("workspaceRoot", out _));
    }

    [Fact]
    public void Exec_maps_wall_time_and_truncation_metadata()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(
                sessionId: 7,
                exitCode: 3,
                terminalText: "partial\n",
                wallTimeSeconds: 0.0042,
                truncated: true),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.ExecSession(new MspNativeExecSessionRequest
        {
            CommandText = "fixture"
        });

        Assert.True(result.Ok);
        Assert.Equal(3, result.ExitCode);
        Assert.Equal(0.0042, result.WallTimeSeconds);
        Assert.True(result.Truncated);
    }

    [Fact]
    public void Write_stdin_empty_poll_maps_kind_and_returns_retained_text()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(
                sessionId: 42,
                exitCode: 0,
                terminalText: "retained output\n"),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.ExecSession(new MspNativeExecSessionRequest
        {
            SessionId = 42
        });

        Assert.True(result.Ok);
        Assert.Equal(42UL, result.SessionId);
        Assert.Equal("retained output\n", result.TerminalText);

        using var request = JsonDocument.Parse(transport.LastRequestJson);
        Assert.Equal("writeStdin", request.RootElement.GetProperty("kind").GetString());
        Assert.Equal(42UL, request.RootElement.GetProperty("sessionId").GetUInt64());
        Assert.False(request.RootElement.TryGetProperty("commandText", out _));
    }

    [Fact]
    public void Write_stdin_nonempty_passes_chars_and_preserves_inactive_session_envelope()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(
                sessionId: 42,
                exitCode: 1,
                terminalText: "write_stdin failed: inactive session 42\n"),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.ExecSession(new MspNativeExecSessionRequest
        {
            SessionId = 42,
            Chars = "data\n"
        });

        Assert.True(result.Ok);
        Assert.Equal(1, result.ExitCode);
        Assert.Equal("write_stdin failed: inactive session 42\n", result.TerminalText);

        using var request = JsonDocument.Parse(transport.LastRequestJson);
        Assert.Equal("writeStdin", request.RootElement.GetProperty("kind").GetString());
        Assert.Equal("data\n", request.RootElement.GetProperty("chars").GetString());
    }

    [Fact]
    public void Ok_false_maps_to_error_result_without_throwing()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(
                ok: false,
                sessionId: 9,
                errorCode: "msp.exec_session.inactive",
                errorMessage: "no such session"),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.ExecSession(new MspNativeExecSessionRequest
        {
            SessionId = 9
        });

        Assert.False(result.Ok);
        Assert.Equal(9UL, result.SessionId);
        Assert.NotNull(result.Error);
        Assert.Equal("msp.exec_session.inactive", result.Error!.Code);
        Assert.Equal("no such session", result.Error.Message);
    }

    [Fact]
    public void Ok_false_canceled_with_token_throws_operation_canceled()
    {
        using var cts = new CancellationTokenSource();
        cts.Cancel();
        var transport = new FakeTransport(
            (_, _) => ExecResult(
                ok: false,
                sessionId: 9,
                canceled: true,
                errorCode: "msp.exec_session.canceled",
                errorMessage: "canceled"),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        Assert.Throws<OperationCanceledException>(() =>
            adapter.ExecSession(
                new MspNativeExecSessionRequest { SessionId = 9 },
                cts.Token));
    }

    [Fact]
    public void Exec_requires_length_delimited_v2_and_exec_sessions_capability()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(),
            new MspNativeRuntimeInfo(
                MspNativeAbiMode.LengthDelimitedV2,
                2,
                0,
                MspNativeContract.AbiV2ContractId,
                MspNativeContract.AbiV2RequiredCapabilities));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "echo hi"
            }));

        Assert.Equal(MspNativeFailureKind.NativeUnsupportedOperation, exception.FailureKind);
        Assert.Equal(MspNativeOperation.ExecSession, exception.Operation);
        Assert.Null(transport.LastOperation);
    }

    [Fact]
    public void Exec_rejects_legacy_v1_transport()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(),
            new MspNativeRuntimeInfo(
                MspNativeAbiMode.LegacyV1,
                0,
                0,
                0,
                0));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "echo hi"
            }));

        Assert.Equal(MspNativeFailureKind.NativeUnsupportedOperation, exception.FailureKind);
        Assert.Null(transport.LastOperation);
    }

    [Fact]
    public void Exec_rejects_empty_command_text_before_invoking()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        Assert.Throws<ArgumentException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "   "
            }));
        Assert.Null(transport.LastOperation);
    }

    [Fact]
    public void Write_stdin_rejects_oversized_chars_before_invoking()
    {
        var transport = new FakeTransport(
            (_, _) => ExecResult(),
            ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        Assert.Throws<ArgumentOutOfRangeException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                SessionId = 3,
                Chars = new string('x', MspNativeExecSessionLimits.MaximumWriteStdinChars + 1)
            }));
        Assert.Null(transport.LastOperation);
    }

    [Fact]
    public void Exec_rejects_unsupported_contract_version()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = "reados-msp-native/999",
            ["ok"] = true,
            ["sessionId"] = 1UL,
            ["running"] = false,
            ["exitCode"] = 0,
            ["wallTimeSeconds"] = 0.0
        });
        var transport = new FakeTransport((_, _) => response, ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "echo hi"
            }));

        Assert.Equal(MspNativeFailureKind.UnsupportedContractVersion, exception.FailureKind);
    }

    [Fact]
    public void Exec_rejects_closed_response_without_exit_code()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["ok"] = true,
            ["sessionId"] = 1UL,
            ["running"] = false,
            ["exitCode"] = null,
            ["wallTimeSeconds"] = 0.0
        });
        var transport = new FakeTransport((_, _) => response, ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "echo hi"
            }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    [Fact]
    public void Exec_rejects_running_response_with_exit_code()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["ok"] = true,
            ["sessionId"] = 1UL,
            ["running"] = true,
            ["exitCode"] = 0,
            ["wallTimeSeconds"] = 0.0
        });
        var transport = new FakeTransport((_, _) => response, ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "echo hi"
            }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    [Fact]
    public void Exec_rejects_ok_false_without_error()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["ok"] = false,
            ["sessionId"] = 1UL,
            ["wallTimeSeconds"] = 0.0,
            ["error"] = null
        });
        var transport = new FakeTransport((_, _) => response, ExecSessionsRuntimeInfo());
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.ExecSession(new MspNativeExecSessionRequest
            {
                CommandText = "echo hi"
            }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    private static MspNativeRuntimeInfo ExecSessionsRuntimeInfo()
    {
        return new MspNativeRuntimeInfo(
            MspNativeAbiMode.LengthDelimitedV2,
            2,
            0,
            MspNativeContract.AbiV2ContractId,
            (ulong)MspNativeAbiV2Capabilities.LengthDelimitedJson |
            (ulong)MspNativeAbiV2Capabilities.Execute |
            (ulong)MspNativeAbiV2Capabilities.Parse |
            (ulong)MspNativeAbiV2Capabilities.Normalize |
            (ulong)MspNativeAbiV2Capabilities.WorkspaceRead |
            (ulong)MspNativeAbiV2Capabilities.ExecSessions);
    }

    private static byte[] ExecResult(
        bool ok = true,
        ulong sessionId = 1,
        bool running = false,
        string? terminalText = null,
        int? exitCode = 0,
        double wallTimeSeconds = 0.001,
        bool truncated = false,
        bool canceled = false,
        string? errorCode = null,
        string? errorMessage = null)
    {
        return Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["ok"] = ok,
            ["sessionId"] = sessionId,
            ["running"] = running,
            ["terminalText"] = terminalText,
            ["exitCode"] = exitCode,
            ["wallTimeSeconds"] = wallTimeSeconds,
            ["truncated"] = truncated,
            ["canceled"] = canceled,
            ["error"] = ok
                ? null
                : new Dictionary<string, object?>
                {
                    ["code"] = errorCode,
                    ["message"] = errorMessage
                }
        });
    }

    private static byte[] Json(object value)
    {
        return JsonSerializer.SerializeToUtf8Bytes(value);
    }

    private sealed class FakeTransport(
        Func<MspNativeOperation, ReadOnlyMemory<byte>, byte[]> handler,
        MspNativeRuntimeInfo? runtimeInfo = null) :
        IMspNativeTransport,
        IMspNativeRuntimeInfoProvider
    {
        public MspNativeRuntimeInfo NativeRuntimeInfo { get; } =
            runtimeInfo ?? MspNativeRuntimeInfo.Unknown;

        public MspNativeOperation? LastOperation { get; private set; }

        public byte[] LastRequestJson { get; private set; } = Array.Empty<byte>();

        public byte[] Invoke(MspNativeOperation operation, ReadOnlyMemory<byte> requestJsonUtf8)
        {
            LastOperation = operation;
            LastRequestJson = requestJsonUtf8.ToArray();
            var response = handler(operation, requestJsonUtf8);
            return response?.ToArray()!;
        }

        public void Dispose()
        {
        }
    }
}

using System.Text;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeCommandResultMapperTests
{
    private readonly MspNativeCommandResultMapper mapper = new();

    [Fact]
    public void Map_projects_utf8_output_and_diagnostics_without_native_audit()
    {
        var nativeDiagnostic = new MspNativeDiagnostic
        {
            Severity = MspNativeDiagnosticSeverity.Warning,
            Code = "msp.native.fixture",
            Message = "fixture warning",
            Target = "echo",
            RecoveryHint = "retry"
        };
        var native = NativeResult(
            exitCode: 7,
            stdout: Encoding.UTF8.GetBytes("输出\n"),
            stderr: Encoding.UTF8.GetBytes("warning\n"),
            diagnostics: [nativeDiagnostic]);

        var result = mapper.Map(native, "echo", ["fixture"]);

        Assert.Equal(7, result.ExitCode);
        Assert.Equal("输出\n", result.Stdout);
        Assert.Equal("warning\n", result.Stderr);
        Assert.Empty(result.AuditRecords);
        Assert.Empty(result.Artifacts);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("msp.native.fixture", diagnostic.Code);
        Assert.Equal("echo", diagnostic.Target);
        Assert.Equal("retry", diagnostic.RecoveryHint);
    }

    [Fact]
    public void Map_rejects_binary_output_that_text_result_cannot_represent()
    {
        var native = NativeResult(stdout: [0x00, 0xff, (byte)'A']);

        var result = mapper.Map(native, "echo", ["fixture"]);

        Assert.Equal(1, result.ExitCode);
        Assert.Empty(result.Stdout);
        Assert.Equal(
            "msp.native.binary_output_not_supported",
            Assert.Single(result.Diagnostics).Code);
        Assert.Empty(result.AuditRecords);
    }

    [Fact]
    public void Map_rejects_runtime_state_change_instead_of_mutating_managed_context()
    {
        var native = NativeResult(
            stateChange: new MspNativeCommandRuntimeStateChange
            {
                CurrentDirectory = "/changed"
            });

        var result = mapper.Map(native, "echo", ["fixture"]);

        Assert.Equal(1, result.ExitCode);
        Assert.Equal(
            "msp.native.unsupported_state_change",
            Assert.Single(result.Diagnostics).Code);
        Assert.Empty(result.AuditRecords);
    }

    [Theory]
    [InlineData("command")]
    [InlineData("arguments")]
    [InlineData("policy")]
    [InlineData("audit-count")]
    public void Map_rejects_execution_evidence_that_does_not_match_preflight(
        string mismatch)
    {
        var auditRecords = mismatch == "audit-count"
            ? Array.Empty<MspNativeAuditRecord>()
            : new[]
            {
                AuditRecord(
                    commandName: mismatch == "command" ? "pwd" : "echo",
                    arguments: mismatch == "arguments" ? ["different"] : ["fixture"],
                    policyKind: mismatch == "policy"
                        ? MspNativePolicyDecisionKind.NotEvaluated
                        : MspNativePolicyDecisionKind.Allow)
            };
        var native = NativeResult(auditRecords: auditRecords);

        var result = mapper.Map(native, "echo", ["fixture"]);

        Assert.Equal(1, result.ExitCode);
        Assert.Equal(
            "msp.native.route.execution_mismatch",
            Assert.Single(result.Diagnostics).Code);
        Assert.Empty(result.AuditRecords);
    }

    private static MspNativeCommandResult NativeResult(
        int exitCode = 0,
        byte[]? stdout = null,
        byte[]? stderr = null,
        MspNativeCommandRuntimeStateChange? stateChange = null,
        IReadOnlyList<MspNativeDiagnostic>? diagnostics = null,
        IReadOnlyList<MspNativeAuditRecord>? auditRecords = null)
    {
        diagnostics ??= Array.Empty<MspNativeDiagnostic>();
        auditRecords ??= [AuditRecord(exitCode: exitCode, diagnostics: diagnostics)];
        return new MspNativeCommandResult(
            exitCode,
            stdout ?? Array.Empty<byte>(),
            stderr ?? Array.Empty<byte>(),
            stateChange,
            auditRecords,
            diagnostics);
    }

    private static MspNativeAuditRecord AuditRecord(
        string commandName = "echo",
        IReadOnlyList<string>? arguments = null,
        MspNativePolicyDecisionKind policyKind = MspNativePolicyDecisionKind.Allow,
        int exitCode = 0,
        IReadOnlyList<MspNativeDiagnostic>? diagnostics = null)
    {
        return new MspNativeAuditRecord
        {
            RunId = "native-run-1",
            CommandLine = "echo fixture",
            CommandName = commandName,
            Arguments = arguments ?? ["fixture"],
            ExitCode = exitCode,
            StartedAtUnixMs = 1,
            EndedAtUnixMs = 2,
            Actor = "tester",
            SessionId = "session-1",
            WorkingDirectory = "/",
            PolicyDecision = new MspNativePolicyDecision
            {
                Kind = policyKind
            },
            Diagnostics = diagnostics ?? []
        };
    }
}

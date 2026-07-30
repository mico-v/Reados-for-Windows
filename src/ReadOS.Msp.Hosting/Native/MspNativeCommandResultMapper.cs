using System.Text;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Native;

public sealed class MspNativeCommandResultMapper
{
    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    public MspCommandResult Map(
        MspNativeCommandResult nativeResult,
        string expectedCommandName,
        IReadOnlyList<string> expectedArguments)
    {
        ArgumentNullException.ThrowIfNull(nativeResult);
        ArgumentException.ThrowIfNullOrWhiteSpace(expectedCommandName);
        ArgumentNullException.ThrowIfNull(expectedArguments);

        if (nativeResult.AuditRecords.Count != 1 ||
            !string.Equals(
                nativeResult.AuditRecords[0].CommandName,
                expectedCommandName,
                StringComparison.Ordinal) ||
            !nativeResult.AuditRecords[0].Arguments.SequenceEqual(
                expectedArguments,
                StringComparer.Ordinal) ||
            nativeResult.AuditRecords[0].PolicyDecision.Kind !=
                MspNativePolicyDecisionKind.Allow)
        {
            return MspCommandResult.Failure(
                "The native execution evidence did not match the managed command and preflight AST.",
                code: "msp.native.route.execution_mismatch",
                target: expectedCommandName,
                recoveryHint: "Install a matching native runtime and retry the canonical command.");
        }

        if (nativeResult.StateChange is not null)
        {
            return MspCommandResult.Failure(
                "The native command returned a runtime state change that this managed host cannot apply.",
                code: "msp.native.unsupported_state_change",
                recoveryHint: "Use a stateless native command until managed runtime-state bridging is available.");
        }

        string stdout;
        string stderr;
        try
        {
            stdout = StrictUtf8.GetString(nativeResult.StdoutBytes.AsSpan());
            stderr = StrictUtf8.GetString(nativeResult.StderrBytes.AsSpan());
        }
        catch (DecoderFallbackException)
        {
            return MspCommandResult.Failure(
                "The native command returned binary output that the managed text result cannot represent.",
                code: "msp.native.binary_output_not_supported",
                recoveryHint: "Use the native byte-stream API until managed command results support authoritative bytes.");
        }

        return new MspCommandResult
        {
            ExitCode = nativeResult.ExitCode,
            Stdout = stdout,
            Stderr = stderr,
            Diagnostics = nativeResult.Diagnostics
                .Select(diagnostic => diagnostic.ToManagedDiagnostic())
                .ToArray(),
            Artifacts = Array.Empty<MspArtifact>(),
            AuditRecords = Array.Empty<MspAuditRecord>()
        };
    }
}

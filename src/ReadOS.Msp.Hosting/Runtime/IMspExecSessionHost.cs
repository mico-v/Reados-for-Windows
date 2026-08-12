using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Runtime;

/// <summary>
/// Host-authorized model-facing exec session surface. Executes a command to
/// completion (returning terminal text plus a session id) and continues an
/// existing session through write_stdin. The model-facing methods format
/// Codex-style terminal text and never surface the raw native JSON.
/// </summary>
public interface IMspExecSessionHost
{
    Task<MspExecSessionRead> ExecCommandAsync(
        string command,
        string? workingDirectory = null,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default);

    Task<MspExecSessionRead> WriteStdinAsync(
        ulong sessionId,
        string? chars = null,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default);
}

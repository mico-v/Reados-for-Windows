using ReadOS.Msp.Hosting.Native;
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
    /// <summary>Runs a typed new exec request.</summary>
    Task<MspExecSessionResponse> ExecAsync(
        MspExecSessionRequest request,
        CancellationToken ct = default);

    /// <summary>Compatibility name for <see cref="ExecAsync"/>.</summary>
    Task<MspExecSessionResponse> ExecCommandAsync(
        MspExecSessionRequest request,
        CancellationToken ct = default);

    /// <summary>Parses and runs strict JSON exec arguments.</summary>
    Task<MspExecSessionResponse> ExecJsonAsync(
        string jsonArguments,
        CancellationToken ct = default);

    /// <summary>Parses and dispatches strict JSON session arguments.</summary>
    Task<MspExecSessionResponse> DispatchJsonAsync(
        string operation,
        string jsonArguments,
        CancellationToken ct = default);

    /// <summary>Parses and dispatches strict JSON stdin arguments.</summary>
    Task<MspExecSessionResponse> WriteStdinJsonAsync(
        string jsonArguments,
        CancellationToken ct = default);

    Task<MspExecSessionRead> ExecCommandAsync(
        string command,
        string? workingDirectory = null,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default);

    Task<MspExecSessionRead> ExecCommandAsync(
        MspExecSessionMode mode,
        string program,
        IReadOnlyList<string>? arguments,
        string? workspaceRoot,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default,
        IReadOnlyDictionary<string, string>? environment = null);

    /// <summary>Polls a retained session without writing stdin.</summary>
    Task<MspExecSessionResponse> PollStdinAsync(
        MspExecSessionStdinPollRequest request,
        CancellationToken ct = default);

    /// <summary>Writes bounded stdin to a retained session.</summary>
    Task<MspExecSessionResponse> ContinueStdinAsync(
        MspExecSessionStdinContinuationRequest request,
        CancellationToken ct = default);

    /// <summary>
    /// Dispatches a common stdin request; null or empty chars means poll.
    /// </summary>
    Task<MspExecSessionResponse> WriteStdinAsync(
        MspExecSessionStdinRequest request,
        CancellationToken ct = default);

    Task<MspExecSessionRead> WriteStdinAsync(
        ulong sessionId,
        string? chars = null,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default);
}

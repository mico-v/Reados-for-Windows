using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Runtime;

/// <summary>
/// Model-facing exec session facade backed by the shared native adapter. The
/// adapter provider is host-authorized: the facade only ever asks the provider
/// for the required adapter and never constructs or owns a native transport.
/// <para>
/// Each native invocation is synchronous and cannot be preempted in flight, so
/// cancellation is checked before and after the call and when the native
/// response reports a canceled session. Model-visible output is formatted
/// Codex-style terminal text; the raw native JSON is never surfaced.
/// </para>
/// </summary>
public sealed class MspExecSessionHostFacade : IMspExecSessionHost
{
    private readonly IMspNativeAdapterProvider adapterProvider;

    public MspExecSessionHostFacade(IMspNativeAdapterProvider adapterProvider)
    {
        this.adapterProvider = adapterProvider ?? throw new ArgumentNullException(nameof(adapterProvider));
    }

    /// <summary>
    /// Runs a command synchronously to completion and returns the model-visible
    /// terminal text plus the retained session id.
    /// </summary>
    public Task<MspExecSessionRead> ExecCommandAsync(
        string command,
        string? workingDirectory = null,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(command);
        ct.ThrowIfCancellationRequested();

        var result = InvokeExecSession(new MspNativeExecSessionRequest
        {
            CommandText = command,
            WorkingDirectory = workingDirectory,
            Actor = "agent",
            YieldTimeMs = yieldTimeMs,
            MaxOutputTokens = maxOutputTokens
        }, ct);

        return Task.FromResult(FormatModelVisible(result));
    }

    /// <summary>
    /// Continues an existing session through write_stdin. An empty or omitted
    /// <paramref name="chars"/> polls the retained terminal text/status once
    /// and closes; a non-empty write to an already-closed session returns the
    /// coordinator-shaped inactive-session closed envelope (exit 1).
    /// </summary>
    public Task<MspExecSessionRead> WriteStdinAsync(
        ulong sessionId,
        string? chars = null,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();

        var result = InvokeExecSession(new MspNativeExecSessionRequest
        {
            SessionId = sessionId,
            Chars = chars,
            Actor = "agent",
            YieldTimeMs = yieldTimeMs,
            MaxOutputTokens = maxOutputTokens
        }, ct);

        return Task.FromResult(FormatModelVisible(result));
    }

    /// <summary>
    /// Runtime/UI read path. Returns the same <see cref="MspExecSessionRead"/>
    /// record without the model-visible empty-poll formatting and timing. It
    /// is for UI synchronization, diagnostics, and oracle runners and must not
    /// be surfaced to the model as a replacement for <c>write_stdin</c>.
    /// </summary>
    public Task<MspExecSessionRead> ReadSession(
        ulong sessionId,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();

        var result = InvokeExecSession(new MspNativeExecSessionRequest
        {
            SessionId = sessionId,
            Actor = "agent",
            YieldTimeMs = yieldTimeMs,
            MaxOutputTokens = maxOutputTokens
        }, ct);

        return Task.FromResult(FormatRuntimeRead(result));
    }

    private MspNativeExecSessionResult InvokeExecSession(
        MspNativeExecSessionRequest request,
        CancellationToken ct)
    {
        var adapter = adapterProvider.GetRequiredAdapter();
        var result = adapter.ExecSession(request, ct);
        ct.ThrowIfCancellationRequested();
        return result;
    }

    private static MspExecSessionRead FormatModelVisible(MspNativeExecSessionResult result)
    {
        if (!result.Ok)
        {
            return new MspExecSessionRead
            {
                SessionId = result.SessionId,
                Running = false,
                Error = result.Error?.Message ?? "The native exec session failed."
            };
        }

        var status = result.Running
            ? $"Process running with session ID {result.SessionId}"
            : $"Process exited with code {result.ExitCode}";
        var terminalText = string.IsNullOrEmpty(result.TerminalText)
            ? $"Wall time: {result.WallTimeSeconds:F4} seconds\n{status}\nOutput:\n"
            : $"Wall time: {result.WallTimeSeconds:F4} seconds\n{status}\nOutput:\n{result.TerminalText}";

        return new MspExecSessionRead
        {
            SessionId = result.SessionId,
            TerminalText = terminalText,
            ExitCode = result.ExitCode,
            Running = result.Running,
            Truncated = result.Truncated
        };
    }

    private static MspExecSessionRead FormatRuntimeRead(MspNativeExecSessionResult result)
    {
        if (!result.Ok)
        {
            return new MspExecSessionRead
            {
                SessionId = result.SessionId,
                Running = false,
                Error = result.Error?.Message ?? "The native exec session read failed."
            };
        }

        return new MspExecSessionRead
        {
            SessionId = result.SessionId,
            TerminalText = result.TerminalText ?? string.Empty,
            ExitCode = result.ExitCode,
            Running = result.Running,
            Truncated = result.Truncated
        };
    }
}

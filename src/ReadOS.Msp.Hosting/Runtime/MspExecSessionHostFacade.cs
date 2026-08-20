using System.Text.Json;
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
    /// Runs a typed new exec request and returns a model-facing response.
    /// </summary>
    public Task<MspExecSessionResponse> ExecAsync(
        MspExecSessionRequest request,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        ct.ThrowIfCancellationRequested();
        ValidateTypedExecRequest(request);

        var result = InvokeExecSession(new MspNativeExecSessionRequest
        {
            Mode = request.Mode,
            CommandText = request.Mode == MspExecSessionMode.Shell
                ? request.ResolvedCommandText
                : null,
            Program = request.Mode == MspExecSessionMode.Process
                ? request.Program
                : null,
            Arguments = request.Mode == MspExecSessionMode.Process
                ? request.Arguments
                : null,
            WorkspaceRoot = request.Mode == MspExecSessionMode.Process
                ? request.WorkspaceRoot
                : null,
            Environment = request.Mode == MspExecSessionMode.Process
                ? request.Environment
                : null,
            WorkingDirectory = request.WorkingDirectory,
            Actor = "agent",
            YieldTimeMs = request.YieldTimeMs,
            MaxOutputTokens = request.MaxOutputTokens
        }, ct);

        return Task.FromResult(ToResponse(
            FormatModelVisible(result, request.WorkspaceRoot)));
    }

    /// <summary>Compatibility name for the typed <see cref="ExecAsync"/>.</summary>
    public Task<MspExecSessionResponse> ExecCommandAsync(
        MspExecSessionRequest request,
        CancellationToken ct = default)
    {
        return ExecAsync(request, ct);
    }

    /// <summary>Parses and runs strict JSON exec arguments.</summary>
    public Task<MspExecSessionResponse> ExecJsonAsync(
        string jsonArguments,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(jsonArguments);
        ct.ThrowIfCancellationRequested();
        return ExecAsync(MspExecSessionJsonArguments.ParseExec(jsonArguments), ct);
    }

    /// <summary>Runs strict JSON exec arguments supplied as a JSON element.</summary>
    public Task<MspExecSessionResponse> ExecJsonAsync(
        JsonElement jsonArguments,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();
        return ExecAsync(MspExecSessionJsonArguments.ParseExec(jsonArguments), ct);
    }

    /// <summary>Compatibility name for the JSON exec entry point.</summary>
    public Task<MspExecSessionResponse> ExecCommandJsonAsync(
        string jsonArguments,
        CancellationToken ct = default)
    {
        return ExecJsonAsync(jsonArguments, ct);
    }

    /// <summary>Compatibility name for the JSON exec element entry point.</summary>
    public Task<MspExecSessionResponse> ExecCommandJsonAsync(
        JsonElement jsonArguments,
        CancellationToken ct = default)
    {
        return ExecJsonAsync(jsonArguments, ct);
    }

    /// JSON argument shapes to their typed facade contracts.
    /// </summary>
    public Task<MspExecSessionResponse> DispatchJsonAsync(
        string operation,
        string jsonArguments,
        CancellationToken ct = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);
        ArgumentNullException.ThrowIfNull(jsonArguments);
        ct.ThrowIfCancellationRequested();

        return operation switch
        {
            "exec_command" => ExecJsonAsync(jsonArguments, ct),
            "write_stdin" => WriteStdinJsonAsync(jsonArguments, ct),
            _ => throw new ArgumentException(
                "The JSON exec-session operation is not supported.",
                nameof(operation))
        };
    }

    /// <summary>Dispatches a JSON argument element to the typed facade.</summary>
    public Task<MspExecSessionResponse> DispatchJsonAsync(
        string operation,
        JsonElement jsonArguments,
        CancellationToken ct = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);
        ct.ThrowIfCancellationRequested();

        return operation switch
        {
            "exec_command" => ExecJsonAsync(jsonArguments, ct),
            "write_stdin" => WriteStdinJsonAsync(jsonArguments, ct),
            _ => throw new ArgumentException(
                "The JSON exec-session operation is not supported.",
                nameof(operation))
        };
    }

    /// <summary>Parses and dispatches strict JSON stdin arguments.</summary>
    public Task<MspExecSessionResponse> WriteStdinJsonAsync(
        string jsonArguments,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(jsonArguments);
        ct.ThrowIfCancellationRequested();
        return WriteStdinAsync(MspExecSessionJsonArguments.ParseStdin(jsonArguments), ct);
    }

    /// <summary>Dispatches strict JSON stdin arguments supplied as an element.</summary>
    public Task<MspExecSessionResponse> WriteStdinJsonAsync(
        JsonElement jsonArguments,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();
        return WriteStdinAsync(MspExecSessionJsonArguments.ParseStdin(jsonArguments), ct);
    }

    /// <summary>Polls a retained session without writing stdin.</summary>
    public Task<MspExecSessionResponse> PollStdinAsync(
        MspExecSessionStdinPollRequest request,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        return WriteTypedStdinAsync(
            request.SessionId,
            chars: null,
            request.YieldTimeMs,
            request.MaxOutputTokens,
            ct);
    }

    /// <summary>Continues a retained session with bounded stdin text.</summary>
    public Task<MspExecSessionResponse> ContinueStdinAsync(
        MspExecSessionStdinContinuationRequest request,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        ArgumentNullException.ThrowIfNull(request.Chars);
        ValidateStdinChars(request.Chars);
        return WriteTypedStdinAsync(
            request.SessionId,
            request.Chars,
            request.YieldTimeMs,
            request.MaxOutputTokens,
            ct);
    }

    /// <summary>
    /// Dispatches a common stdin request. Null or empty chars performs an
    /// empty poll; non-empty chars continue the retained session.
    /// </summary>
    public Task<MspExecSessionResponse> WriteStdinAsync(
        MspExecSessionStdinRequest request,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        if (request.IsPoll)
        {
            return WriteTypedStdinAsync(
                request.SessionId,
                chars: null,
                request.YieldTimeMs,
                request.MaxOutputTokens,
                ct);
        }

        ValidateStdinChars(request.Chars!);
        return WriteTypedStdinAsync(
            request.SessionId,
            request.Chars,
            request.YieldTimeMs,
            request.MaxOutputTokens,
            ct);
    }

    private Task<MspExecSessionResponse> WriteTypedStdinAsync(
        ulong sessionId,
        string? chars,
        int? yieldTimeMs,
        int? maxOutputTokens,
        CancellationToken ct)
    {
        if (sessionId == 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(sessionId),
                "A stdin session id must be non-zero.");
        }

        ct.ThrowIfCancellationRequested();
        var result = InvokeExecSession(new MspNativeExecSessionRequest
        {
            SessionId = sessionId,
            Chars = chars,
            Actor = "agent",
            YieldTimeMs = yieldTimeMs,
            MaxOutputTokens = maxOutputTokens
        }, ct);

        return Task.FromResult(ToResponse(FormatModelVisible(result)));
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
    /// Runs an explicit process-mode exec synchronously to completion. Process
    /// mode launches <paramref name="program"/> with bounded
    /// <paramref name="arguments"/> inside the host-authorized
    /// <paramref name="workspaceRoot"/>; shell mode (the default) routes the
    /// program text through the host shell exactly like the command overload.
    /// The optional <paramref name="environment"/> is forwarded only for a new
    /// process-mode session; it is host-authorized input and never appears in
    /// the model-visible read.
    /// </summary>
    public Task<MspExecSessionRead> ExecCommandAsync(
        MspExecSessionMode mode,
        string program,
        IReadOnlyList<string>? arguments,
        string? workspaceRoot,
        int? yieldTimeMs = null,
        int? maxOutputTokens = null,
        CancellationToken ct = default,
        IReadOnlyDictionary<string, string>? environment = null)
    {
        ArgumentNullException.ThrowIfNull(program);
        ct.ThrowIfCancellationRequested();

        var result = InvokeExecSession(new MspNativeExecSessionRequest
        {
            Mode = mode,
            CommandText = mode == MspExecSessionMode.Shell ? program : null,
            Program = mode == MspExecSessionMode.Process ? program : null,
            Arguments = mode == MspExecSessionMode.Process ? arguments : null,
            WorkspaceRoot = mode == MspExecSessionMode.Process ? workspaceRoot : null,
            Environment = mode == MspExecSessionMode.Process ? environment : null,
            Actor = "agent",
            YieldTimeMs = yieldTimeMs,
            MaxOutputTokens = maxOutputTokens
        }, ct);

        return Task.FromResult(FormatModelVisible(
            result,
            mode == MspExecSessionMode.Process ? workspaceRoot : null));
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

    private static void ValidateTypedExecRequest(MspExecSessionRequest request)
    {
        if (!Enum.IsDefined(request.Mode))
        {
            throw new ArgumentOutOfRangeException(
                nameof(request),
                "The exec session mode is not defined.");
        }

        if (request.YieldTimeMs is < 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(request),
                "YieldTimeMs must be non-negative.");
        }

        if (request.MaxOutputTokens is < 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(request),
                "MaxOutputTokens must be non-negative.");
        }

        if (request.Mode == MspExecSessionMode.Process)
        {
            if (request.Environment is { Count: > MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries })
            {
                throw new ArgumentOutOfRangeException(
                    nameof(request),
                    $"Process-mode exec sessions support at most {MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries} environment entries.");
            }

            if (request.Environment is not null)
            {
                foreach (var entry in request.Environment)
                {
                    if (entry.Key is null || entry.Value is null)
                    {
                        throw new ArgumentException(
                            "Process-mode exec session environment entries must not be null.",
                            nameof(request));
                    }

                    var entryBytes = checked(
                        System.Text.Encoding.UTF8.GetByteCount(entry.Key) +
                        System.Text.Encoding.UTF8.GetByteCount(entry.Value));
                    if (entryBytes > MspNativeExecSessionLimits.MaximumProcessEnvironmentEntryBytes)
                    {
                        throw new ArgumentOutOfRangeException(
                            nameof(request),
                            $"Process-mode exec session environment entries must not exceed {MspNativeExecSessionLimits.MaximumProcessEnvironmentEntryBytes} UTF-8 bytes combined.");
                    }
                }
            }
        }
    }

    private static void ValidateStdinChars(string chars)
    {
        if (chars.Length == 0)
        {
            throw new ArgumentException(
                "A stdin continuation requires non-empty chars.",
                nameof(chars));
        }

        if (chars.Length > MspNativeExecSessionLimits.MaximumWriteStdinChars)
        {
            throw new ArgumentOutOfRangeException(
                nameof(chars),
                $"Stdin chars must not exceed {MspNativeExecSessionLimits.MaximumWriteStdinChars} characters.");
        }
    }

    private static MspExecSessionResponse ToResponse(MspExecSessionRead read)
    {
        return new MspExecSessionResponse
        {
            SessionId = read.SessionId,
            TerminalText = read.TerminalText,
            ExitCode = read.ExitCode,
            Running = read.Running,
            Truncated = read.Truncated,
            Error = read.Error
        };
    }

    private static MspExecSessionRead FormatModelVisible(
        MspNativeExecSessionResult result,
        string? workspaceRoot = null)
    {
        if (!result.Ok)
        {
            return new MspExecSessionRead
            {
                SessionId = result.SessionId,
                Running = false,
                Error = SanitizeError(
                    result.Error?.Message,
                    "The native exec session failed.",
                    workspaceRoot)
            };
        }

        var status = result.Running
            ? $"Process running with session ID {result.SessionId}"
            : $"Process exited with code {result.ExitCode}";
        var visibleTerminalText = ContainsHostPath(result.TerminalText, workspaceRoot)
            ? null
            : result.TerminalText;
        var terminalText = string.IsNullOrEmpty(visibleTerminalText)
            ? $"Wall time: {result.WallTimeSeconds:F4} seconds\n{status}\nOutput:\n"
            : $"Wall time: {result.WallTimeSeconds:F4} seconds\n{status}\nOutput:\n{visibleTerminalText}";

        return new MspExecSessionRead
        {
            SessionId = result.SessionId,
            TerminalText = terminalText,
            ExitCode = result.ExitCode,
            Running = result.Running,
            Truncated = result.Truncated
        };
    }

    private static string SanitizeError(
        string? message,
        string fallback,
        string? workspaceRoot)
    {
        if (string.IsNullOrEmpty(message))
        {
            return fallback;
        }

        if (ContainsHostPath(message, workspaceRoot))
        {
            return fallback;
        }

        return message;
    }

    private static bool ContainsHostPath(
        string? value,
        string? workspaceRoot)
    {
        if (string.IsNullOrEmpty(value) || string.IsNullOrWhiteSpace(workspaceRoot))
        {
            return false;
        }

        var slashPath = workspaceRoot.Replace('\\', '/');
        var backslashPath = workspaceRoot.Replace('/', '\\');
        return value.Contains(workspaceRoot, StringComparison.OrdinalIgnoreCase) ||
            value.Contains(slashPath, StringComparison.OrdinalIgnoreCase) ||
            value.Contains(backslashPath, StringComparison.OrdinalIgnoreCase);
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

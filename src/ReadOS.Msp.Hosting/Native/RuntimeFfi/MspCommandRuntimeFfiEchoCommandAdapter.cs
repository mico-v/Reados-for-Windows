using System.Text;
using System.Text.RegularExpressions;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Models;
using ReadOS.Msp.Parsing;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Native.RuntimeFfi;

/// <summary>
/// Opt-in managed command adapter for the narrow, canonical <c>echo</c> slice of
/// the command-runtime FFI. It deliberately returns an ordinary managed result;
/// the containing <see cref="MspRuntime"/> remains responsible for policy and
/// terminal audit ownership.
/// </summary>
public sealed class MspCommandRuntimeFfiEchoCommandAdapter : IMspCommand, IDisposable
{
    public const string AdapterUnavailableDiagnosticCode = "msp.runtime_ffi.adapter_unavailable";
    public const string InvalidCommandDiagnosticCode = "msp.runtime_ffi.invalid_echo_command";
    public const string ExecutionDiagnosticCode = "msp.runtime_ffi.execution_failed";
    public const string BinaryOutputDiagnosticCode = "msp.runtime_ffi.binary_output";
    public const string DefaultVirtualCwd = "/workspace";

    private static readonly Encoding StrictUtf8 = new UTF8Encoding(false, true);
    private static readonly Regex WindowsAbsolutePath = new(
        @"^(?:[A-Za-z]:[\\/]|\\\\|//)",
        RegexOptions.CultureInvariant | RegexOptions.Compiled,
        TimeSpan.FromSeconds(1));
    private static readonly Regex WindowsAbsolutePathInCommand = new(
        @"(?:^|[\s'""`])(?:[A-Za-z]:[\\/]|\\\\|//)",
        RegexOptions.CultureInvariant | RegexOptions.Compiled,
        TimeSpan.FromSeconds(1));

    private readonly IMspCommandRuntimeFfiAdapter? adapter;
    private readonly string virtualCwd;
    private readonly bool ownsAdapter;
    private int disposed;

    /// <summary>
    /// Creates an opt-in adapter. A null adapter is allowed so callers can
    /// represent an unavailable optional installation as a typed command result
    /// rather than falling back to another execution route.
    /// </summary>
    public MspCommandRuntimeFfiEchoCommandAdapter(
        IMspCommandRuntimeFfiAdapter? adapter,
        string virtualCwd = DefaultVirtualCwd,
        bool ownsAdapter = false)
    {
        if (string.IsNullOrWhiteSpace(virtualCwd) ||
            !MspCommandRuntimeFfiUtf8.IsValidVirtualPath(
                virtualCwd,
                allowRoot: false,
                MspCommandRuntimeFfiLimits.DefaultMaximumVirtualPathBytes))
        {
            throw new ArgumentException(
                "The FFI echo adapter requires a non-root virtual cwd.",
                nameof(virtualCwd));
        }

        this.adapter = adapter;
        this.virtualCwd = virtualCwd;
        this.ownsAdapter = ownsAdapter;
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref disposed, 1) == 0 && ownsAdapter)
        {
            adapter?.Dispose();
        }
    }

    public string Name => "echo";

    public string Summary => "Write arguments to stdout through the opt-in runtime FFI.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "echo [text...]");

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments) => Metadata;

    /// <summary>
    /// Executes the command through the normal managed command contract. The
    /// invocation's host working directory, actor, environment, policy, and
    /// audit state are intentionally not copied into the FFI request.
    /// </summary>
    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(context);
        ArgumentNullException.ThrowIfNull(arguments);

        return ExecuteValidatedAsync(
            new MspCommandRuntimeFfiEchoRequest
            {
                CommandText = context.Invocation.CommandText
            },
            arguments,
            cancellationToken);
    }

    /// <summary>
    /// Executes an explicit echo request for callers that have an intentionally
    /// supplied virtual stdin or variable map. No host environment is consulted.
    /// </summary>
    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandRuntimeFfiEchoRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        return ExecuteValidatedAsync(
            request,
            parsedArguments: null,
            cancellationToken);
    }

    private async ValueTask<MspCommandResult> ExecuteValidatedAsync(
        MspCommandRuntimeFfiEchoRequest request,
        IReadOnlyList<string>? parsedArguments,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();

        if (adapter is null)
        {
            return UnavailableResult();
        }

        if (!TryClassifySupportedEcho(
                request.CommandText,
                parsedArguments,
                out var arguments,
                out var rejection))
        {
            return rejection!;
        }

        cancellationToken.ThrowIfCancellationRequested();

        try
        {
            using var runtime = adapter.CreateRuntime();
            using var workspace = adapter.CreateWorkspace();
            using var nativeResult = adapter.Execute(
                runtime,
                workspace,
                new MspCommandRuntimeFfiRequest
                {
                    Command = request.CommandText,
                    Cwd = virtualCwd,
                    StdinBytes = request.StdinBytes,
                    Variables = request.Variables,
                    ErrorOnUnbound = true
                });

            cancellationToken.ThrowIfCancellationRequested();
            return MapResult(nativeResult, arguments);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (MspCommandRuntimeFfiException exception)
        {
            return MapFfiFailure(exception);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            return MspCommandResult.Failure(
                "The command runtime FFI echo invocation failed.",
                code: ExecutionDiagnosticCode,
                target: Name,
                recoveryHint: "Verify the explicitly configured runtime FFI library and retry.");
        }
    }

    private MspCommandResult MapResult(
        MspCommandRuntimeFfiResult nativeResult,
        IReadOnlyList<string> arguments)
    {
        string stdout;
        string stderr;
        try
        {
            stdout = StrictUtf8.GetString(nativeResult.StdoutBytes);
            stderr = StrictUtf8.GetString(nativeResult.StderrBytes);
        }
        catch (DecoderFallbackException)
        {
            return MspCommandResult.Failure(
                "The runtime FFI echo result was not valid UTF-8.",
                code: BinaryOutputDiagnosticCode,
                target: Name,
                recoveryHint: "Use the managed echo route for output that is not valid UTF-8.");
        }

        if (nativeResult.ExitCode == 0)
        {
            return new MspCommandResult
            {
                ExitCode = 0,
                Stdout = stdout,
                Stderr = stderr,
                Diagnostics = Array.Empty<MspCommandDiagnostic>(),
                AuditRecords = Array.Empty<MspAuditRecord>()
            };
        }

        var code = string.IsNullOrWhiteSpace(nativeResult.DiagnosticCode)
            ? ExecutionDiagnosticCode
            : nativeResult.DiagnosticCode;
        return new MspCommandResult
        {
            ExitCode = nativeResult.ExitCode,
            Stdout = stdout,
            Stderr = stderr,
            Diagnostics = new[]
            {
                MspCommandDiagnostic.Error(
                    code,
                    string.IsNullOrWhiteSpace(stderr)
                        ? "The runtime FFI echo command failed."
                        : stderr,
                    Name,
                    "Use the managed echo route or retry with a supported canonical command.")
            },
            AuditRecords = Array.Empty<MspAuditRecord>()
        };
    }

    private static bool TryClassifySupportedEcho(
        string commandText,
        IReadOnlyList<string>? expectedArguments,
        out IReadOnlyList<string> arguments,
        out MspCommandResult? rejection)
    {
        arguments = Array.Empty<string>();
        rejection = null;

        if (string.IsNullOrWhiteSpace(commandText) ||
            commandText.IndexOfAny(['\r', '\n']) >= 0 ||
            !HasExactEchoCommandToken(commandText))
        {
            rejection = InvalidCommand(
                "Only an exact lowercase echo command is eligible for the runtime FFI route.");
            return false;
        }

        MspParsedCommand parsed;
        try
        {
            parsed = MspCommandLineParser.Parse(commandText);
        }
        catch (MspParseException exception)
        {
            rejection = InvalidCommand(exception.Message);
            return false;
        }

        if (!string.Equals(parsed.Name, "echo", StringComparison.Ordinal) ||
            ContainsUnsupportedOperator(commandText) ||
            ContainsUnsupportedExpansion(commandText) ||
            WindowsAbsolutePathInCommand.IsMatch(commandText) ||
            parsed.Arguments.Any(IsHostAbsolutePath))
        {
            rejection = InvalidCommand(
                "The runtime FFI echo route accepts only a simple canonical command without expansions or host paths.");
            return false;
        }

        if (expectedArguments is not null &&
            !parsed.Arguments.SequenceEqual(expectedArguments, StringComparer.Ordinal))
        {
            rejection = InvalidCommand(
                "The managed parser arguments did not match the requested echo command.");
            return false;
        }

        arguments = parsed.Arguments;
        return true;
    }

    private static bool HasExactEchoCommandToken(string commandText)
    {
        var start = 0;
        while (start < commandText.Length && char.IsWhiteSpace(commandText[start]))
        {
            start++;
        }

        return commandText.AsSpan(start).StartsWith("echo", StringComparison.Ordinal) &&
            (start + 4 == commandText.Length || char.IsWhiteSpace(commandText[start + 4]));
    }

    private static bool ContainsUnsupportedOperator(string commandText)
    {
        var inSingleQuote = false;
        var inDoubleQuote = false;
        var escaping = false;
        foreach (var character in commandText)
        {
            if (escaping)
            {
                escaping = false;
                continue;
            }

            if (character == '\\' && !inSingleQuote)
            {
                escaping = true;
                continue;
            }

            if (character == '\'' && !inDoubleQuote)
            {
                inSingleQuote = !inSingleQuote;
                continue;
            }

            if (character == '"' && !inSingleQuote)
            {
                inDoubleQuote = !inDoubleQuote;
                continue;
            }

            if (!inSingleQuote && !inDoubleQuote && character is '|' or ';' or '<' or '>' or '&')
            {
                return true;
            }
        }

        return false;
    }

    private static bool ContainsUnsupportedExpansion(string commandText)
    {
        var inSingleQuote = false;
        var inDoubleQuote = false;
        var escaping = false;
        foreach (var character in commandText)
        {
            if (escaping)
            {
                escaping = false;
                continue;
            }

            if (character == '\\' && !inSingleQuote)
            {
                escaping = true;
                continue;
            }

            if (character == '\'' && !inDoubleQuote)
            {
                inSingleQuote = !inSingleQuote;
                continue;
            }

            if (character == '"' && !inSingleQuote)
            {
                inDoubleQuote = !inDoubleQuote;
                continue;
            }

            if (!inSingleQuote && character is '$' or '`')
            {
                return true;
            }
        }

        return false;
    }

    private static bool IsHostAbsolutePath(string value) =>
        WindowsAbsolutePath.IsMatch(value);

    private static MspCommandResult InvalidCommand(string message) => MspCommandResult.Failure(
        message,
        exitCode: 2,
        code: InvalidCommandDiagnosticCode,
        target: "echo",
        recoveryHint: "Use exact lowercase echo with literal arguments and no shell operators, expansions, or host paths.");

    private static MspCommandResult UnavailableResult() => MspCommandResult.Failure(
        "The explicitly configured command runtime FFI adapter is unavailable.",
        code: AdapterUnavailableDiagnosticCode,
        target: "echo",
        recoveryHint: "Configure a matching runtime FFI library before selecting this opt-in route.");

    private static MspCommandResult MapFfiFailure(MspCommandRuntimeFfiException exception)
    {
        var code = exception.FailureKind == MspCommandRuntimeFfiFailureKind.LibraryUnavailable
            ? AdapterUnavailableDiagnosticCode
            : $"msp.runtime_ffi.{ToDiagnosticSuffix(exception.FailureKind)}";
        return MspCommandResult.Failure(
            code == AdapterUnavailableDiagnosticCode
                ? "The explicitly configured command runtime FFI adapter is unavailable."
                : "The command runtime FFI echo invocation failed.",
            code: code,
            target: "echo",
            recoveryHint: "Verify the explicitly configured runtime FFI library and retry.");
    }

    private static string ToDiagnosticSuffix(MspCommandRuntimeFfiFailureKind kind) =>
        kind switch
        {
            MspCommandRuntimeFfiFailureKind.InvalidArgument => "invalid_argument",
            MspCommandRuntimeFfiFailureKind.LimitExceeded => "limit_exceeded",
            MspCommandRuntimeFfiFailureKind.NativePanic => "native_panic",
            MspCommandRuntimeFfiFailureKind.Disposed => "disposed",
            _ => "execution_failed"
        };

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

/// <summary>Explicit input accepted by the opt-in FFI echo adapter.</summary>
public sealed record MspCommandRuntimeFfiEchoRequest
{
    public required string CommandText { get; init; }

    public byte[]? StdinBytes { get; init; }

    public IReadOnlyDictionary<string, string>? Variables { get; init; }
}

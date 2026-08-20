using System.Text;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Uses managed command metadata and outer-runtime policy/audit while executing
/// an eligible canonical pwd, echo, ls, or cat command through the native MSP
/// adapter. Read-only ls/cat access is limited to the managed virtual-workspace
/// callback contract; no host filesystem root is sent to native code.
/// </summary>
public sealed class MspNativeBackedCommand : IMspCommand
{
    private static readonly HashSet<string> SupportedCommandNames = new(
        ["pwd", "echo", "ls", "cat"],
        StringComparer.Ordinal);

    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private const ulong NativeReadRangeLimitBytes = 1024 * 1024;
    private const ulong ReadOnlyBackendId = 1;

    private readonly IMspCommand managedCommandDefinition;
    private readonly IMspNativeAdapterProvider adapterProvider;
    private readonly MspNativeCommandRouteClassifier routeClassifier;
    private readonly MspNativeCommandResultMapper resultMapper;

    public MspNativeBackedCommand(
        IMspCommand managedCommandDefinition,
        IMspNativeAdapterProvider adapterProvider,
        MspNativeCommandRouteClassifier? routeClassifier = null,
        MspNativeCommandResultMapper? resultMapper = null)
    {
        this.managedCommandDefinition = managedCommandDefinition ??
            throw new ArgumentNullException(nameof(managedCommandDefinition));
        this.adapterProvider = adapterProvider ?? throw new ArgumentNullException(nameof(adapterProvider));
        this.routeClassifier = routeClassifier ?? new MspNativeCommandRouteClassifier();
        this.resultMapper = resultMapper ?? new MspNativeCommandResultMapper();

        if (!SupportedCommandNames.Contains(managedCommandDefinition.Name))
        {
            throw new ArgumentException(
                "The native adoption v1 route is restricted to canonical lowercase pwd, echo, ls, and cat commands.",
                nameof(managedCommandDefinition));
        }
    }

    public string Name => managedCommandDefinition.Name;

    public string Summary => managedCommandDefinition.Summary;

    public MspCommandMetadata Metadata => managedCommandDefinition.Metadata;

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        return managedCommandDefinition.GetMetadata(arguments);
    }

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        return managedCommandDefinition.GetPreview(arguments);
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(context);
        ArgumentNullException.ThrowIfNull(arguments);
        cancellationToken.ThrowIfCancellationRequested();

        var invocation = context.Invocation;
        if (!string.Equals(invocation.CommandName, Name, StringComparison.Ordinal))
        {
            return ValueTask.FromResult(MspCommandResult.Failure(
                "The selected managed command did not match the native-backed command.",
                exitCode: 2,
                code: "msp.native.route.managed_command_mismatch",
                target: Name,
                recoveryHint: "Run the exact canonical lowercase pwd, echo, ls, or cat command."));
        }

        if (invocation.DryRun)
        {
            return ValueTask.FromResult(MspCommandResult.Success(
                $"dry-run: {invocation.CommandText}"));
        }

        IMspNativeAdapter adapter;
        try
        {
            adapter = adapterProvider.GetRequiredAdapter();
        }
        catch (MspNativeAdapterException exception)
        {
            return ValueTask.FromResult(MapAdapterFailure(exception));
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            return ValueTask.FromResult(MapAdapterFailure(
                MspNativeAdapterException.Create(
                    MspNativeFailureKind.LibraryUnavailable,
                null)));
        }
        cancellationToken.ThrowIfCancellationRequested();

        MspNativeShellParseResult parseResult;
        try
        {
            parseResult = adapter.Parse(new MspNativeShellParseRequest
            {
                CommandText = invocation.CommandText
            });
        }
        catch (MspNativeAdapterException exception)
        {
            return ValueTask.FromResult(MapAdapterFailure(exception));
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            return ValueTask.FromResult(MapAdapterFailure(
                MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    MspNativeOperation.Parse)));
        }
        cancellationToken.ThrowIfCancellationRequested();

        var route = routeClassifier.Classify(
            Name,
            invocation.CommandText,
            arguments,
            parseResult);
        if (!route.ShouldExecuteNative)
        {
            return ValueTask.FromResult(MspCommandResult.Failure(
                route.Message,
                exitCode: 2,
                code: route.DiagnosticCode,
                target: Name,
                recoveryHint: route.RecoveryHint));
        }

        cancellationToken.ThrowIfCancellationRequested();
        if (Name is "ls" or "cat")
        {
            return ExecuteReadOnlyAsync(
                adapter,
                context,
                arguments,
                cancellationToken);
        }

        try
        {
            var nativeResult = adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = invocation.CommandText,
                WorkingDirectory = context.WorkingDirectory,
                Actor = invocation.Actor,
                SessionId = invocation.SessionId,
                DryRun = false,
                WorkspaceRoot = null
            });
            cancellationToken.ThrowIfCancellationRequested();
            return ValueTask.FromResult(resultMapper.Map(
                nativeResult,
                Name,
                arguments));
        }
        catch (MspNativeAdapterException exception)
        {
            return ValueTask.FromResult(MapAdapterFailure(exception));
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            return ValueTask.FromResult(MapAdapterFailure(
                MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    MspNativeOperation.Execute)));
        }
    }

    private async ValueTask<MspCommandResult> ExecuteReadOnlyAsync(
        IMspNativeAdapter adapter,
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (context.Workspace is not IMspNativeReadOnlyWorkspace workspace)
        {
            return MspCommandResult.Failure(
                "The managed virtual workspace does not expose the native read-only callback surface.",
                code: "msp.native.workspace_unavailable",
                target: Name,
                recoveryHint: "Use the managed workspace host that provides virtual read callbacks.");
        }

        MspNativeWorkspaceInvocation workspaceInvocation;
        try
        {
            workspaceInvocation = new MspNativeWorkspaceInvocation(
                callbackBase: new MspNativeWorkspaceBackend
                {
                    Id = ReadOnlyBackendId,
                    Workspace = workspace
                });
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            return MspCommandResult.Failure(
                "The managed virtual workspace could not be attached to the native read-only route.",
                code: "msp.native.workspace_unavailable",
                target: Name,
                recoveryHint: "Use the managed workspace host that provides virtual read callbacks.");
        }

        return Name == "ls"
            ? await ExecuteVirtualListAsync(
                adapter,
                workspaceInvocation,
                context,
                arguments,
                cancellationToken)
            : await ExecuteVirtualCatAsync(
                adapter,
                workspaceInvocation,
                context,
                arguments,
                cancellationToken);
    }

    private async ValueTask<MspCommandResult> ExecuteVirtualListAsync(
        IMspNativeAdapter adapter,
        MspNativeWorkspaceInvocation workspaceInvocation,
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        var operand = arguments.Count == 0 ? context.WorkingDirectory : arguments[0];
        if (!TryNormalizeVirtualOperand(context, operand, out var normalized))
        {
            return MspCommandResult.Failure("No such file or directory: /", exitCode: 2);
        }

        if (!IsSafeVirtualPath(normalized))
        {
            return MspCommandResult.Failure("No such file or directory: /", exitCode: 2);
        }

        try
        {
            var nativeResult = adapter.ReadWorkspace(
                workspaceInvocation,
                MspNativeWorkspaceReadOperation.ListDirectory,
                normalized,
                cancellationToken: cancellationToken);
            cancellationToken.ThrowIfCancellationRequested();
            if (nativeResult is not MspNativeWorkspaceReadResult.EntriesResult entriesResult)
            {
                return MspCommandResult.Failure(
                    "The native workspace returned an unexpected listing result.",
                    code: "msp.native.workspace_result_mismatch",
                    target: Name,
                    recoveryHint: "Retry with the managed virtual workspace route.");
            }

            var builder = new StringBuilder();
            foreach (var entry in entriesResult.Entries.OrderBy(item => item.Name, StringComparer.OrdinalIgnoreCase))
            {
                if (!IsSafeVirtualEntryName(entry.Name))
                {
                    return MspCommandResult.Failure(
                        "The native workspace returned an invalid virtual entry.",
                        code: "msp.native.workspace_result_mismatch",
                        target: Name,
                        recoveryHint: "Retry with the managed virtual workspace route.");
                }

                builder.Append(entry.Info.FileType == MspNativeWorkspaceFileType.Directory ? "d " : "- ");
                builder.Append(entry.Name);
                if (entry.Info.SizeBytes is ulong size)
                {
                    builder.Append('\t');
                    builder.Append(size);
                }

                builder.AppendLine();
            }

            return MspCommandResult.Success(builder.ToString());
        }
        catch (MspNativeWorkspaceException exception)
        {
            return MapWorkspaceFailure(exception, normalized, isCat: false);
        }
        catch (MspNativeAdapterException exception)
        {
            return MapAdapterFailure(exception);
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            return MspCommandResult.Failure(
                "The native workspace listing failed.",
                code: "msp.native.workspace_invoke_failed",
                target: Name,
                recoveryHint: "Retry with the managed virtual workspace route.");
        }
    }

    private async ValueTask<MspCommandResult> ExecuteVirtualCatAsync(
        IMspNativeAdapter adapter,
        MspNativeWorkspaceInvocation workspaceInvocation,
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (arguments.Count == 0)
        {
            return MspCommandResult.Failure("Usage: cat <path>", exitCode: 2);
        }

        var builder = new StringBuilder();
        foreach (var operand in arguments)
        {
            if (!TryNormalizeVirtualOperand(context, operand, out var normalized))
            {
                return MspCommandResult.Failure("Cannot read text file: /");
            }

            if (!IsSafeVirtualPath(normalized))
            {
                return MspCommandResult.Failure("Cannot read text file: /");
            }

            try
            {
                var statResult = adapter.ReadWorkspace(
                    workspaceInvocation,
                    MspNativeWorkspaceReadOperation.Stat,
                    normalized,
                    cancellationToken: cancellationToken);
                if (statResult is not MspNativeWorkspaceReadResult.FileInfoResult fileInfoResult)
                {
                    return MspCommandResult.Failure(
                        "The native workspace returned an unexpected stat result.",
                        code: "msp.native.workspace_result_mismatch",
                        target: Name,
                        recoveryHint: "Retry with the managed virtual workspace route.");
                }

                if (fileInfoResult.FileInfo.FileType != MspNativeWorkspaceFileType.RegularFile ||
                    fileInfoResult.FileInfo.SizeBytes is not ulong size)
                {
                    return MspCommandResult.Failure($"Cannot read text file: {normalized}");
                }

                if (size > NativeReadRangeLimitBytes || size > int.MaxValue)
                {
                    return MspCommandResult.Failure(
                        "The native workspace file exceeds the read-only route limit.",
                        code: "msp.native.workspace_limit_exceeded",
                        target: normalized,
                        recoveryHint: "Use a smaller virtual text file or the managed workspace route.");
                }

                var readResult = adapter.ReadWorkspace(
                    workspaceInvocation,
                    MspNativeWorkspaceReadOperation.ReadFileRange,
                    normalized,
                    length: (int)size,
                    cancellationToken: cancellationToken);
                if (readResult is not MspNativeWorkspaceReadResult.BytesResult bytesResult)
                {
                    return MspCommandResult.Failure(
                        "The native workspace returned an unexpected file result.",
                        code: "msp.native.workspace_result_mismatch",
                        target: Name,
                        recoveryHint: "Retry with the managed virtual workspace route.");
                }

                string content;
                try
                {
                    content = StrictUtf8.GetString(bytesResult.Bytes.Span);
                }
                catch (DecoderFallbackException)
                {
                    return MspCommandResult.Failure(
                        "The native command returned binary output that the managed text result cannot represent.",
                        code: "msp.native.binary_output_not_supported",
                        target: normalized,
                        recoveryHint: "Use the native byte-stream API until managed command results support authoritative bytes.");
                }

                builder.Append(content);
                if (!content.EndsWith(Environment.NewLine, StringComparison.Ordinal))
                {
                    builder.AppendLine();
                }
            }
            catch (MspNativeWorkspaceException exception)
            {
                return MapWorkspaceFailure(exception, normalized, isCat: true);
            }
            catch (MspNativeAdapterException exception)
            {
                return MapAdapterFailure(exception);
            }
            catch (Exception exception) when (
                exception is not OperationCanceledException && !IsFatal(exception))
            {
                return MspCommandResult.Failure(
                    "The native workspace read failed.",
                    code: "msp.native.workspace_invoke_failed",
                    target: Name,
                    recoveryHint: "Retry with the managed virtual workspace route.");
            }

            cancellationToken.ThrowIfCancellationRequested();
        }

        return MspCommandResult.Success(builder.ToString());
    }

    private static bool TryNormalizeVirtualOperand(
        MspCommandContext context,
        string operand,
        out string normalized)
    {
        try
        {
            normalized = context.Workspace.NormalizePath(operand, context.WorkingDirectory);
            return true;
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            normalized = "/";
            return false;
        }
    }

    private static bool IsSafeVirtualPath(string path)
    {
        return path.StartsWith("/", StringComparison.Ordinal) &&
            !path.Contains('\\') &&
            !path.Contains(':') &&
            !path.Contains('\0') &&
            !path
                .Split('/', StringSplitOptions.RemoveEmptyEntries)
                .Any(component => component is "." or ".." ||
                    string.Equals(component, ".msp", StringComparison.OrdinalIgnoreCase));
    }

    private static bool IsSafeVirtualEntryName(string name)
    {
        return !string.IsNullOrWhiteSpace(name) &&
            name is not "." and not ".." &&
            !name.Contains('/') &&
            !name.Contains('\\') &&
            !name.Contains(':') &&
            !name.Contains('\0') &&
            !name.Any(char.IsControl) &&
            !string.Equals(name, ".msp", StringComparison.OrdinalIgnoreCase);
    }

    private static MspCommandResult MapWorkspaceFailure(
        MspNativeWorkspaceException _,
        string normalized,
        bool isCat)
    {
        return isCat
            ? MspCommandResult.Failure($"Cannot read text file: {normalized}")
            : MspCommandResult.Failure($"No such file or directory: {normalized}", exitCode: 2);
    }

    private static MspCommandResult MapAdapterFailure(MspNativeAdapterException exception)
    {
        var diagnostic = exception.Diagnostic.ToManagedDiagnostic();
        return new MspCommandResult
        {
            ExitCode = 1,
            Stderr = diagnostic.Message + Environment.NewLine,
            Diagnostics = [diagnostic],
            AuditRecords = Array.Empty<MspAuditRecord>(),
            Artifacts = Array.Empty<MspArtifact>()
        };
    }

    private static bool IsFatal(Exception exception)
    {
        return exception is OutOfMemoryException or StackOverflowException;
    }
}

using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Uses managed command metadata and outer-runtime policy/audit while executing
/// an eligible canonical pwd or echo command through the native MSP adapter.
/// The provider is shared and remains owned by the host composition.
/// </summary>
public sealed class MspNativeBackedCommand : IMspCommand
{
    private static readonly HashSet<string> SupportedCommandNames = new(
        ["pwd", "echo"],
        StringComparer.Ordinal);

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
                "The native adoption v1 route is restricted to canonical lowercase pwd and echo commands.",
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
                recoveryHint: "Run the exact canonical lowercase pwd or echo command."));
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

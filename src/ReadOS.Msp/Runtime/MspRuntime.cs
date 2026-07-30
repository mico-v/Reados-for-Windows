using System.Runtime.CompilerServices;
using System.Threading.Channels;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Models;
using ReadOS.Msp.Parsing;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Runtime;

public sealed class MspRuntime
{
    private readonly MspCommandContext context;

    public MspRuntime(MspCommandContext context)
    {
        this.context = context ?? throw new ArgumentNullException(nameof(context));
    }

    public static MspRuntime CreateDefault(IMspWorkspace? workspace = null)
    {
        var registry = CreateDefaultRegistry();
        var resolvedWorkspace = workspace ?? new InMemoryMspWorkspace();
        var context = new MspCommandContext(
            resolvedWorkspace,
            registry,
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
        return new MspRuntime(context);
    }

    public static MspCommandRegistry CreateDefaultRegistry()
    {
        var registry = new MspCommandRegistry();
        registry
            .Register(new PwdCommand())
            .Register(new EchoCommand())
            .Register(new LsCommand())
            .Register(new CatCommand())
            .Register(new ArtifactCommand())
            .Register(new WorkflowCommand());
        registry.Register(new HelpCommand(registry));
        return registry;
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        return ExecuteSafelyAsync(request, eventSink: null, cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        return ExecuteStreamingCoreAsync(request, cancellationToken);
    }

    private async IAsyncEnumerable<MspCommandEvent> ExecuteStreamingCoreAsync(
        MspCommandRequest request,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var channel = Channel.CreateUnbounded<MspCommandEvent>(new UnboundedChannelOptions
        {
            SingleReader = true,
            SingleWriter = false
        });
        var eventSink = new ChannelMspCommandEventSink(channel.Writer);
        var execution = Task.Run(async () =>
        {
            try
            {
                await ExecuteSafelyAsync(request, eventSink, cancellationToken);
            }
            finally
            {
                channel.Writer.TryComplete();
            }
        }, CancellationToken.None);

        await foreach (var commandEvent in channel.Reader.ReadAllAsync())
        {
            yield return commandEvent;
        }

        await execution;
    }

    private async ValueTask<MspCommandResult> ExecuteSafelyAsync(
        MspCommandRequest request,
        IMspCommandEventSink? eventSink,
        CancellationToken cancellationToken)
    {
        try
        {
            return await ExecuteCoreAsync(request, eventSink, cancellationToken);
        }
        catch (OperationCanceledException)
        {
            var canceled = CreateCanceledResult();
            return await CompleteWithAuditAsync(
                request,
                CreateRequestContext(request, eventSink),
                string.Empty,
                CreateUnresolvedMetadata(string.Empty),
                MspCommandPreview.Empty,
                MspPolicyDecision.NotEvaluated,
                canceled,
                eventSink,
                MspCommandEventKind.Canceled);
        }
        catch (Exception ex)
        {
            var failed = MspCommandResult.Failure(
                ex.Message,
                code: "msp.runtime.exception",
                recoveryHint: "Inspect the command and runtime host state before retrying.");
            return await CompleteWithAuditAsync(
                request,
                CreateRequestContext(request, eventSink),
                string.Empty,
                CreateUnresolvedMetadata(string.Empty),
                MspCommandPreview.Empty,
                MspPolicyDecision.NotEvaluated,
                failed,
                eventSink,
                MspCommandEventKind.Completed);
        }
    }

    private async ValueTask<MspCommandResult> ExecuteCoreAsync(
        MspCommandRequest request,
        IMspCommandEventSink? eventSink,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var requestContext = CreateRequestContext(request, eventSink);
        MspParsedCommand parsed;
        try
        {
            parsed = MspCommandLineParser.Parse(request.CommandText);
        }
        catch (MspParseException ex)
        {
            var parsedFailure = MspCommandResult.Failure(
                ex.Message,
                exitCode: 2,
                code: "msp.parse",
                recoveryHint: "Check quoting, command name, and reserved shell syntax.");
            return await CompleteWithAuditAsync(
                request,
                requestContext,
                string.Empty,
                CreateUnresolvedMetadata(string.Empty),
                MspCommandPreview.Empty,
                MspPolicyDecision.NotEvaluated,
                parsedFailure,
                eventSink,
                MspCommandEventKind.Completed);
        }

        requestContext = requestContext.WithInvocation(new MspCommandInvocation
            {
                Actor = request.Actor,
                CommandText = request.CommandText,
                CommandName = parsed.Name,
                SessionId = request.SessionId,
                DryRun = request.DryRun,
                StartedAt = DateTimeOffset.UtcNow
            });
        await PublishEventAsync(eventSink, new MspCommandEvent
        {
            Kind = MspCommandEventKind.Started,
            Actor = request.Actor,
            SessionId = request.SessionId,
            CommandText = request.CommandText,
            CommandName = parsed.Name,
            Message = "Command execution started."
        }, CancellationToken.None);

        if (!requestContext.Registry.TryGet(parsed.Name, out var command))
        {
            var missing = MspCommandResult.Failure(
                $"Command not found: {parsed.Name}",
                exitCode: 127,
                code: "msp.command_not_found",
                target: parsed.Name,
                recoveryHint: "Run help to list available MSP commands.");
            return await CompleteWithAuditAsync(
                request,
                requestContext,
                parsed.Name,
                CreateUnresolvedMetadata(parsed.Name),
                MspCommandPreview.Empty,
                MspPolicyDecision.NotEvaluated,
                missing,
                eventSink,
                MspCommandEventKind.Completed);
        }

        var commandMetadata = command.GetMetadata(parsed.Arguments);
        var commandPreview = command.GetPreview(parsed.Arguments);
        MspPolicyDecision decision;
        try
        {
            decision = await requestContext.Policy.AuthorizeAsync(new MspPolicyRequest
            {
                CommandName = parsed.Name,
                CommandText = request.CommandText,
                Actor = request.Actor,
                SessionId = request.SessionId,
                WorkingDirectory = requestContext.WorkingDirectory,
                DryRun = request.DryRun,
                Environment = request.Environment,
                Arguments = parsed.Arguments,
                CommandMetadata = commandMetadata,
                Preview = commandPreview
            }, cancellationToken);
        }
        catch (OperationCanceledException)
        {
            return await CompleteWithAuditAsync(
                request,
                requestContext,
                parsed.Name,
                commandMetadata,
                commandPreview,
                MspPolicyDecision.NotEvaluated,
                CreateCanceledResult(),
                eventSink,
                MspCommandEventKind.Canceled);
        }
        catch (Exception ex)
        {
            var policyFailure = MspCommandResult.Failure(
                ex.Message,
                code: "msp.policy.exception",
                target: parsed.Name,
                recoveryHint: "Inspect the host policy configuration before retrying the command.");
            return await CompleteWithAuditAsync(
                request,
                requestContext,
                parsed.Name,
                commandMetadata,
                commandPreview,
                MspPolicyDecision.NotEvaluated,
                policyFailure,
                eventSink,
                MspCommandEventKind.Completed);
        }
        await PublishEventAsync(eventSink, new MspCommandEvent
        {
            Kind = MspCommandEventKind.PolicyDecision,
            Actor = request.Actor,
            SessionId = request.SessionId,
            CommandText = request.CommandText,
            CommandName = parsed.Name,
            Decision = decision,
            Effects = commandMetadata.Effects,
            Preview = commandPreview,
            Message = $"Policy decision: {decision}"
        }, CancellationToken.None);

        if (decision != MspPolicyDecision.Allow)
        {
            var denied = MspCommandResult.Failure(
                $"Policy decision: {decision}",
                exitCode: 126,
                code: GetPolicyDiagnosticCode(decision),
                target: parsed.Name,
                recoveryHint: GetPolicyRecoveryHint(decision));
            return await CompleteWithAuditAsync(
                request,
                requestContext,
                parsed.Name,
                commandMetadata,
                commandPreview,
                decision,
                denied,
                eventSink,
                MspCommandEventKind.Completed);
        }

        MspCommandResult result;
        try
        {
            cancellationToken.ThrowIfCancellationRequested();
            result = request.DryRun
                ? MspCommandResult.Success($"dry-run: {request.CommandText}")
                : await command.ExecuteAsync(requestContext, parsed.Arguments, cancellationToken);
        }
        catch (OperationCanceledException)
        {
            return await CompleteWithAuditAsync(
                request,
                requestContext,
                parsed.Name,
                commandMetadata,
                commandPreview,
                decision,
                CreateCanceledResult(),
                eventSink,
                MspCommandEventKind.Canceled);
        }
        catch (Exception ex)
        {
            result = MspCommandResult.Failure(
                ex.Message,
                code: "msp.exception",
                target: parsed.Name,
                recoveryHint: "Inspect stderr, command arguments, and workspace state before retrying.");
        }

        return await CompleteWithAuditAsync(
            request,
            requestContext,
            parsed.Name,
            commandMetadata,
            commandPreview,
            decision,
            result,
            eventSink,
            MspCommandEventKind.Completed);
    }

    private MspCommandContext CreateRequestContext(
        MspCommandRequest request,
        IMspCommandEventSink? eventSink)
    {
        return context
            .WithEventSink(eventSink)
            .WithWorkingDirectory(request.WorkingDirectory);
    }

    private static MspCommandMetadata CreateUnresolvedMetadata(string commandName)
    {
        return MspCommandMetadata.Create(
            commandName,
            string.IsNullOrWhiteSpace(commandName)
                ? "Command text could not be parsed."
                : "Command is not registered.");
    }

    private static MspCommandResult CreateCanceledResult()
    {
        return MspCommandResult.Failure(
            "Command execution was canceled.",
            exitCode: 130,
            code: "msp.canceled",
            recoveryHint: "Rerun the command if the canceled operation is still needed.");
    }

    private static async ValueTask<MspCommandResult> CompleteWithAuditAsync(
        MspCommandRequest request,
        MspCommandContext context,
        string commandName,
        MspCommandMetadata commandMetadata,
        MspCommandPreview commandPreview,
        MspPolicyDecision decision,
        MspCommandResult result,
        IMspCommandEventSink? eventSink,
        MspCommandEventKind terminalKind)
    {
        var auditRecord = await RecordAuditAsync(
            request,
            context,
            commandName,
            commandMetadata,
            commandPreview,
            decision,
            result,
            CancellationToken.None);
        var finalResult = result with
        {
            AuditRecords = result.AuditRecords.Concat(new[] { auditRecord }).ToArray()
        };
        await PublishTerminalAsync(eventSink, request, commandName, finalResult, terminalKind);
        return finalResult;
    }

    private static async ValueTask<MspAuditRecord> RecordAuditAsync(
        MspCommandRequest request,
        MspCommandContext context,
        string commandName,
        MspCommandMetadata commandMetadata,
        MspCommandPreview commandPreview,
        MspPolicyDecision decision,
        MspCommandResult result,
        CancellationToken cancellationToken)
    {
        var record = new MspAuditRecord
        {
            Actor = request.Actor,
            SessionId = request.SessionId,
            CommandName = commandName,
            CommandText = request.CommandText,
            Decision = decision,
            Effects = commandMetadata.Effects,
            Preview = commandPreview,
            ExitCode = result.ExitCode,
            WorkingDirectory = context.WorkingDirectory,
            Message = result.Succeeded ? null : result.Stderr,
            Diagnostics = result.Diagnostics
        };
        await context.Audit.RecordAsync(record, cancellationToken);
        return record;
    }

    private static ValueTask PublishTerminalAsync(
        IMspCommandEventSink? eventSink,
        MspCommandRequest request,
        string commandName,
        MspCommandResult result,
        MspCommandEventKind terminalKind)
    {
        return PublishEventAsync(eventSink, new MspCommandEvent
        {
            Kind = terminalKind,
            Actor = request.Actor,
            SessionId = request.SessionId,
            CommandText = request.CommandText,
            CommandName = commandName,
            ExitCode = result.ExitCode,
            Result = result,
            Message = terminalKind == MspCommandEventKind.Canceled
                ? "Command execution was canceled."
                : result.Succeeded ? "Command execution completed." : result.Stderr
        }, CancellationToken.None);
    }

    private static ValueTask PublishEventAsync(
        IMspCommandEventSink? eventSink,
        MspCommandEvent commandEvent,
        CancellationToken cancellationToken)
    {
        return eventSink is null
            ? ValueTask.CompletedTask
            : eventSink.PublishAsync(commandEvent, cancellationToken);
    }

    private static string GetPolicyDiagnosticCode(MspPolicyDecision decision)
    {
        return decision switch
        {
            MspPolicyDecision.RequireConfirmation => "msp.policy.require_confirmation",
            MspPolicyDecision.Deny => "msp.policy.denied",
            _ => "msp.policy"
        };
    }

    private static string GetPolicyRecoveryHint(MspPolicyDecision decision)
    {
        return decision switch
        {
            MspPolicyDecision.RequireConfirmation => "Approve the pending command in the workbench transcript, or replay it with a host approval token.",
            MspPolicyDecision.Deny => "Revise the command or host policy before retrying.",
            _ => "Review the policy decision before retrying."
        };
    }

    private sealed class ChannelMspCommandEventSink : IMspCommandEventSink
    {
        private readonly ChannelWriter<MspCommandEvent> writer;

        public ChannelMspCommandEventSink(ChannelWriter<MspCommandEvent> writer)
        {
            this.writer = writer;
        }

        public ValueTask PublishAsync(MspCommandEvent commandEvent, CancellationToken cancellationToken = default)
        {
            return writer.WriteAsync(commandEvent, cancellationToken);
        }

    }
}

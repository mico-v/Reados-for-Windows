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
        this.context = context;
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
            .Register(new ArtifactCommand());
        registry.Register(new HelpCommand(registry));
        return registry;
    }

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        return await ExecuteCoreAsync(request, eventSink: null, cancellationToken);
    }

    public async IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
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
                await ExecuteCoreAsync(request, eventSink, cancellationToken);
            }
            catch (OperationCanceledException)
            {
                eventSink.TryPublish(new MspCommandEvent
                {
                    Kind = MspCommandEventKind.Canceled,
                    Actor = request.Actor,
                    SessionId = request.SessionId,
                    CommandText = request.CommandText,
                    Message = "Command execution was canceled."
                });
            }
            catch (Exception ex)
            {
                eventSink.TryPublish(new MspCommandEvent
                {
                    Kind = MspCommandEventKind.Completed,
                    Actor = request.Actor,
                    SessionId = request.SessionId,
                    CommandText = request.CommandText,
                    ExitCode = 1,
                    Message = ex.Message
                });
            }
            finally
            {
                channel.Writer.TryComplete();
            }
        }, CancellationToken.None);

        await foreach (var commandEvent in channel.Reader.ReadAllAsync(cancellationToken))
        {
            yield return commandEvent;
        }

        await execution;
    }

    private async ValueTask<MspCommandResult> ExecuteCoreAsync(
        MspCommandRequest request,
        IMspCommandEventSink? eventSink,
        CancellationToken cancellationToken)
    {
        MspParsedCommand parsed;
        try
        {
            parsed = MspCommandLineParser.Parse(request.CommandText);
        }
        catch (MspParseException ex)
        {
            return MspCommandResult.Failure(ex.Message, exitCode: 2);
        }

        var requestContext = context
            .WithEventSink(eventSink)
            .WithWorkingDirectory(request.WorkingDirectory)
            .WithInvocation(new MspCommandInvocation
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
        }, cancellationToken);

        if (!requestContext.Registry.TryGet(parsed.Name, out var command))
        {
            var missing = MspCommandResult.Failure($"Command not found: {parsed.Name}", exitCode: 127);
            await PublishCompletedAsync(eventSink, request, parsed.Name, missing, cancellationToken);
            return missing;
        }

        var commandMetadata = command.GetMetadata(parsed.Arguments);
        var commandPreview = command.GetPreview(parsed.Arguments);
        var decision = await requestContext.Policy.AuthorizeAsync(new MspPolicyRequest
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
        }, cancellationToken);

        if (decision != MspPolicyDecision.Allow)
        {
            var denied = MspCommandResult.Failure($"Policy decision: {decision}", exitCode: 126);
            var deniedAuditRecord = await RecordAuditAsync(request, requestContext, parsed.Name, commandMetadata, commandPreview, decision, denied, cancellationToken);
            var deniedResult = denied with { AuditRecords = new[] { deniedAuditRecord } };
            await PublishCompletedAsync(eventSink, request, parsed.Name, deniedResult, cancellationToken);
            return deniedResult;
        }

        MspCommandResult result;
        try
        {
            result = request.DryRun
                ? MspCommandResult.Success($"dry-run: {request.CommandText}")
                : await command.ExecuteAsync(requestContext, parsed.Arguments, cancellationToken);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception ex)
        {
            result = MspCommandResult.Failure(ex.Message);
        }

        var auditRecord = await RecordAuditAsync(request, requestContext, parsed.Name, commandMetadata, commandPreview, decision, result, cancellationToken);
        var finalResult = result with { AuditRecords = result.AuditRecords.Concat(new[] { auditRecord }).ToArray() };
        await PublishCompletedAsync(eventSink, request, parsed.Name, finalResult, cancellationToken);
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
            Message = result.Succeeded ? null : result.Stderr
        };
        await context.Audit.RecordAsync(record, cancellationToken);
        return record;
    }

    private static ValueTask PublishCompletedAsync(
        IMspCommandEventSink? eventSink,
        MspCommandRequest request,
        string commandName,
        MspCommandResult result,
        CancellationToken cancellationToken)
    {
        return PublishEventAsync(eventSink, new MspCommandEvent
        {
            Kind = MspCommandEventKind.Completed,
            Actor = request.Actor,
            SessionId = request.SessionId,
            CommandText = request.CommandText,
            CommandName = commandName,
            ExitCode = result.ExitCode,
            Message = result.Succeeded ? "Command execution completed." : result.Stderr
        }, cancellationToken);
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

        public void TryPublish(MspCommandEvent commandEvent)
        {
            writer.TryWrite(commandEvent);
        }
    }
}

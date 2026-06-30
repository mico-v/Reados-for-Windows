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
            .WithWorkingDirectory(request.WorkingDirectory)
            .WithInvocation(new MspCommandInvocation
            {
                Actor = request.Actor,
                CommandText = request.CommandText,
                SessionId = request.SessionId,
                DryRun = request.DryRun,
                StartedAt = DateTimeOffset.UtcNow
            });
        if (!requestContext.Registry.TryGet(parsed.Name, out var command))
        {
            return MspCommandResult.Failure($"Command not found: {parsed.Name}", exitCode: 127);
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

        if (decision != MspPolicyDecision.Allow)
        {
            var denied = MspCommandResult.Failure($"Policy decision: {decision}", exitCode: 126);
            var deniedAuditRecord = await RecordAuditAsync(request, requestContext, parsed.Name, commandMetadata, commandPreview, decision, denied, cancellationToken);
            return denied with { AuditRecords = new[] { deniedAuditRecord } };
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
        return result with { AuditRecords = result.AuditRecords.Concat(new[] { auditRecord }).ToArray() };
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
}

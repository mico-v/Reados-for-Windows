using ReadOS.App.Models;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Services.Msp;

public sealed class ReadOsMspHost
{
    private const string ApprovalTokenKey = "reados.msp.approvalToken";
    private const string DefaultSessionId = "reados-workbench";

    private readonly MspRuntime runtime;
    private readonly OperatorApprovalMspPolicy policy;

    public ReadOsMspHost(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        IAiChatService aiChatService,
        Func<WorkspaceState?> workspaceProvider,
        Func<WorkspaceSettings> settingsProvider,
        Func<LibraryItem?> selectedDocumentProvider,
        Func<IReadOnlyList<ChatAttachment>> pendingAttachmentsProvider,
        Func<ChatAttachment, Task<string>> attachmentTextProvider,
        Action<ChatAttachment> attachmentSink,
        Action clearAttachments,
        Action<LibraryItem, ChatConversation> chatResultSink)
    {
        var registry = MspRuntime.CreateDefaultRegistry();
        var workspace = new ReadOsVirtualWorkspace(workspaceStore, pdfService, workspaceProvider);
        registry
            .Register(new ReadOsWorkspaceCommand(workspaceStore, workspaceProvider))
            .Register(new ReadOsLibraryCommand(workspaceProvider))
            .Register(new ReadOsPdfCommand(workspaceStore, pdfService, workspaceProvider, selectedDocumentProvider))
            .Register(new ReadOsWindowsCommand(workspaceStore, selectedDocumentProvider))
            .Register(new ReadOsPageLabelCommand(workspaceStore, workspaceProvider, selectedDocumentProvider))
            .Register(new ReadOsOutlineCommand(workspaceStore, workspaceProvider, selectedDocumentProvider))
            .Register(new ReadOsAttachCommand(workspaceProvider, selectedDocumentProvider, attachmentSink))
            .Register(new ReadOsChatCommand(
                workspaceStore,
                aiChatService,
                workspaceProvider,
                settingsProvider,
                selectedDocumentProvider,
                pendingAttachmentsProvider,
                attachmentTextProvider,
                clearAttachments,
                chatResultSink));

        policy = new OperatorApprovalMspPolicy();
        var context = new MspCommandContext(
            workspace,
            registry,
            policy,
            new InMemoryMspAuditSink());
        runtime = new MspRuntime(context);
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = actor,
            SessionId = DefaultSessionId,
            CommandText = commandText
        }, cancellationToken);
    }

    public async ValueTask<MspCommandResult> ExecuteApprovedAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        var approvalToken = policy.ApproveNextCommand(commandText, actor);
        try
        {
            return await runtime.ExecuteAsync(new MspCommandRequest
            {
                Actor = actor,
                SessionId = DefaultSessionId,
                CommandText = commandText,
                Environment = new Dictionary<string, string>
                {
                    [ApprovalTokenKey] = approvalToken
                }
            }, cancellationToken);
        }
        finally
        {
            policy.RevokeApprovalToken(approvalToken);
        }
    }

    private sealed class OperatorApprovalMspPolicy : IMspPolicy
    {
        private readonly EffectBasedMspPolicy effectPolicy = new();
        private readonly Dictionary<string, ApprovedMspCommand> approvalTokens = new(StringComparer.Ordinal);
        private readonly object gate = new();

        public string ApproveNextCommand(string commandText, string actor)
        {
            var token = Guid.NewGuid().ToString("N");
            lock (gate)
            {
                approvalTokens[token] = new ApprovedMspCommand(commandText, actor);
            }

            return token;
        }

        public void RevokeApprovalToken(string token)
        {
            lock (gate)
            {
                approvalTokens.Remove(token);
            }
        }

        public ValueTask<MspPolicyDecision> AuthorizeAsync(
            MspPolicyRequest request,
            CancellationToken cancellationToken = default)
        {
            if (request.Environment.TryGetValue(ApprovalTokenKey, out var token) &&
                ConsumeApprovalToken(token, request))
            {
                return ValueTask.FromResult(MspPolicyDecision.Allow);
            }

            return effectPolicy.AuthorizeAsync(request, cancellationToken);
        }

        private bool ConsumeApprovalToken(string token, MspPolicyRequest request)
        {
            lock (gate)
            {
                if (!approvalTokens.TryGetValue(token, out var approval) ||
                    !string.Equals(approval.CommandText, request.CommandText, StringComparison.Ordinal) ||
                    !string.Equals(approval.Actor, request.Actor, StringComparison.Ordinal))
                {
                    return false;
                }

                approvalTokens.Remove(token);
                return true;
            }
        }

        private sealed record ApprovedMspCommand(string CommandText, string Actor);
    }
}

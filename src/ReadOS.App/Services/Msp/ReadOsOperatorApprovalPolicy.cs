using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsOperatorApprovalPolicy : IMspPolicy
{
    private readonly Func<WorkspaceSettings> settingsProvider;
    private readonly EffectBasedMspPolicy effectPolicy = new();
    private readonly IMspApprovalGrantStore approvalGrants;

    public ReadOsOperatorApprovalPolicy(Func<WorkspaceSettings> settingsProvider)
        : this(settingsProvider, new MspApprovalGrantStore())
    {
    }

    public ReadOsOperatorApprovalPolicy(
        Func<WorkspaceSettings> settingsProvider,
        IMspApprovalGrantStore approvalGrants)
    {
        this.settingsProvider = settingsProvider;
        this.approvalGrants = approvalGrants;
    }

    public string ApproveNextCommand(string commandText, string actor)
    {
        return approvalGrants.ApproveNextCommand(commandText, actor);
    }

    public IReadOnlyDictionary<string, string> CreateApprovalEnvironment(string token)
    {
        return approvalGrants.CreateApprovalEnvironment(token);
    }

    public void RevokeApprovalToken(string token)
    {
        approvalGrants.RevokeApproval(token);
    }

    public ValueTask<MspPolicyDecision> AuthorizeAsync(
        MspPolicyRequest request,
        CancellationToken cancellationToken = default)
    {
        if (approvalGrants.TryConsumeApproval(request))
        {
            return ValueTask.FromResult(MspPolicyDecision.Allow);
        }

        var approvalMode = MspApprovalModeCodes.Normalize(settingsProvider().MspApprovalMode);
        if (approvalMode == MspApprovalModeCodes.ConfirmAll)
        {
            return ValueTask.FromResult(request.DryRun
                ? MspPolicyDecision.Allow
                : MspPolicyDecision.RequireConfirmation);
        }

        if (approvalMode == MspApprovalModeCodes.AllowWorkspace)
        {
            return ValueTask.FromResult(AuthorizeWorkspaceWriteMode(request));
        }

        return effectPolicy.AuthorizeAsync(request, cancellationToken);
    }

    private static MspPolicyDecision AuthorizeWorkspaceWriteMode(MspPolicyRequest request)
    {
        if (request.DryRun)
        {
            return MspPolicyDecision.Allow;
        }

        if (HasAny(request.Effects, MspCommandEffects.DeleteWorkspace | MspCommandEffects.ExternalNetwork | MspCommandEffects.ExternalModel))
        {
            return MspPolicyDecision.RequireConfirmation;
        }

        return MspPolicyDecision.Allow;
    }

    private static bool HasAny(MspCommandEffects effects, MspCommandEffects flags)
    {
        return (effects & flags) != 0;
    }

}

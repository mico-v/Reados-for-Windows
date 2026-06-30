using ReadOS.Msp.Models;

namespace ReadOS.Msp.Policy;

public sealed class EffectBasedMspPolicy : IMspPolicy
{
    public bool RequireConfirmationForMutatingCommands { get; init; } = true;

    public bool DenyExternalEffects { get; init; }

    public ValueTask<MspPolicyDecision> AuthorizeAsync(
        MspPolicyRequest request,
        CancellationToken cancellationToken = default)
    {
        if (request.DryRun)
        {
            return ValueTask.FromResult(MspPolicyDecision.Allow);
        }

        if (DenyExternalEffects && HasAny(request.Effects, MspCommandEffects.ExternalNetwork | MspCommandEffects.ExternalModel))
        {
            return ValueTask.FromResult(MspPolicyDecision.Deny);
        }

        if (RequireConfirmationForMutatingCommands && request.RequiresConfirmation)
        {
            return ValueTask.FromResult(MspPolicyDecision.RequireConfirmation);
        }

        return ValueTask.FromResult(MspPolicyDecision.Allow);
    }

    private static bool HasAny(MspCommandEffects effects, MspCommandEffects flags)
    {
        return (effects & flags) != 0;
    }
}

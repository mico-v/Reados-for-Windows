namespace ReadOS.Msp.Policy;

public sealed class AllowAllMspPolicy : IMspPolicy
{
    public ValueTask<MspPolicyDecision> AuthorizeAsync(MspPolicyRequest request, CancellationToken cancellationToken = default)
    {
        return ValueTask.FromResult(MspPolicyDecision.Allow);
    }
}

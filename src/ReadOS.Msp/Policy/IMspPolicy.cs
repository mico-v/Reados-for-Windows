namespace ReadOS.Msp.Policy;

public interface IMspPolicy
{
    ValueTask<MspPolicyDecision> AuthorizeAsync(MspPolicyRequest request, CancellationToken cancellationToken = default);
}

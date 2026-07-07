using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Hosting.Policy;

public interface IMspApprovalGrantStore
{
    string EnvironmentKey { get; }

    string ApproveNextCommand(string commandText, string actor);

    IReadOnlyDictionary<string, string> CreateApprovalEnvironment(string token);

    void RevokeApproval(string token);

    bool TryConsumeApproval(MspPolicyRequest request);
}

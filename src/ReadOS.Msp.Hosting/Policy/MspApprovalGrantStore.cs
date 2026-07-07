using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Hosting.Policy;

public sealed class MspApprovalGrantStore : IMspApprovalGrantStore
{
    public const string DefaultEnvironmentKey = "reados.msp.approvalToken";

    private readonly Dictionary<string, ApprovedMspCommand> approvalTokens = new(StringComparer.Ordinal);
    private readonly object gate = new();

    public MspApprovalGrantStore(string environmentKey = DefaultEnvironmentKey)
    {
        EnvironmentKey = string.IsNullOrWhiteSpace(environmentKey)
            ? DefaultEnvironmentKey
            : environmentKey.Trim();
    }

    public string EnvironmentKey { get; }

    public string ApproveNextCommand(string commandText, string actor)
    {
        var token = Guid.NewGuid().ToString("N");
        lock (gate)
        {
            approvalTokens[token] = new ApprovedMspCommand(commandText, NormalizeActor(actor));
        }

        return token;
    }

    public IReadOnlyDictionary<string, string> CreateApprovalEnvironment(string token)
    {
        return new Dictionary<string, string>
        {
            [EnvironmentKey] = NormalizeToken(token)
        };
    }

    public void RevokeApproval(string token)
    {
        var normalizedToken = NormalizeToken(token);
        if (normalizedToken.Length == 0)
        {
            return;
        }

        lock (gate)
        {
            approvalTokens.Remove(normalizedToken);
        }
    }

    public bool TryConsumeApproval(MspPolicyRequest request)
    {
        if (!request.Environment.TryGetValue(EnvironmentKey, out var token))
        {
            return false;
        }

        var normalizedToken = NormalizeToken(token);
        if (normalizedToken.Length == 0)
        {
            return false;
        }

        lock (gate)
        {
            if (!approvalTokens.TryGetValue(normalizedToken, out var approval) ||
                !string.Equals(approval.CommandText, request.CommandText, StringComparison.Ordinal) ||
                !string.Equals(approval.Actor, NormalizeActor(request.Actor), StringComparison.Ordinal))
            {
                return false;
            }

            approvalTokens.Remove(normalizedToken);
            return true;
        }
    }

    private static string NormalizeActor(string actor)
    {
        return string.IsNullOrWhiteSpace(actor)
            ? "agent"
            : actor.Trim();
    }

    private static string NormalizeToken(string? token)
    {
        return token?.Trim() ?? string.Empty;
    }

    private sealed record ApprovedMspCommand(string CommandText, string Actor);
}

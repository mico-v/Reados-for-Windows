using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Hosting.Tests.Policy;

public sealed class MspApprovalGrantStoreTests
{
    [Fact]
    public void Approval_grant_allows_exact_command_and_actor_once()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "artifact write /artifacts/notes.md \"notes\"";
        const string actor = "test-agent";
        var token = store.ApproveNextCommand(commandText, actor);
        var environment = store.CreateApprovalEnvironment(token);
        var request = CreateRequest(commandText, actor) with { Environment = environment };

        var first = store.TryConsumeApproval(request);
        var second = store.TryConsumeApproval(request);

        Assert.True(first);
        Assert.False(second);
    }

    [Fact]
    public void Approval_grant_is_bound_to_matching_command_and_actor_before_consumption()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "artifact write /artifacts/notes.md \"notes\"";
        const string actor = "test-agent";
        var token = store.ApproveNextCommand(commandText, actor);
        var environment = store.CreateApprovalEnvironment(token);

        var mismatchedCommand = CreateRequest(
            "artifact write /artifacts/other.md \"notes\"",
            actor) with { Environment = environment };
        var mismatchedActor = CreateRequest(
            commandText,
            "other-agent") with { Environment = environment };
        var matchingRequest = CreateRequest(commandText, actor) with { Environment = environment };

        Assert.False(store.TryConsumeApproval(mismatchedCommand));
        Assert.False(store.TryConsumeApproval(mismatchedActor));
        Assert.True(store.TryConsumeApproval(matchingRequest));
    }

    [Fact]
    public void Approval_grant_can_be_revoked_before_consumption()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "workspace info";
        var token = store.ApproveNextCommand(commandText, "test-agent");
        var request = CreateRequest(commandText, "test-agent") with
        {
            Environment = store.CreateApprovalEnvironment(token)
        };

        store.RevokeApproval(token);

        Assert.False(store.TryConsumeApproval(request));
    }

    [Fact]
    public void Approval_environment_uses_configured_key()
    {
        var store = new MspApprovalGrantStore("custom.approval");
        var token = store.ApproveNextCommand("workspace info", "test-agent");

        var environment = store.CreateApprovalEnvironment(token);

        Assert.Equal("custom.approval", store.EnvironmentKey);
        Assert.True(environment.ContainsKey("custom.approval"));
        Assert.DoesNotContain(MspApprovalGrantStore.DefaultEnvironmentKey, environment.Keys);
    }

    [Fact]
    public void Approval_environment_trims_configured_key()
    {
        var store = new MspApprovalGrantStore(" custom.approval ");
        var token = store.ApproveNextCommand("workspace info", "test-agent");

        var environment = store.CreateApprovalEnvironment(token);

        Assert.Equal("custom.approval", store.EnvironmentKey);
        Assert.True(environment.ContainsKey("custom.approval"));
    }

    [Fact]
    public void Approval_environment_trims_token_value()
    {
        var store = new MspApprovalGrantStore();
        var token = store.ApproveNextCommand("workspace info", "test-agent");

        var environment = store.CreateApprovalEnvironment($" {token} ");

        Assert.Equal(token, environment[MspApprovalGrantStore.DefaultEnvironmentKey]);
    }

    [Fact]
    public void Approval_grant_matches_normalized_actor_metadata()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "artifact write /artifacts/notes.md \"notes\"";

        var paddedGrantToken = store.ApproveNextCommand(commandText, " test-agent ");
        var paddedRequestToken = store.ApproveNextCommand(commandText, "test-agent");

        var trimmedRequest = CreateRequest(commandText, "test-agent") with
        {
            Environment = store.CreateApprovalEnvironment(paddedGrantToken)
        };
        var paddedRequest = CreateRequest(commandText, " test-agent ") with
        {
            Environment = store.CreateApprovalEnvironment(paddedRequestToken)
        };

        Assert.True(store.TryConsumeApproval(trimmedRequest));
        Assert.True(store.TryConsumeApproval(paddedRequest));
    }

    [Fact]
    public void Approval_grant_uses_default_actor_for_empty_actor_metadata()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "workspace info";
        var token = store.ApproveNextCommand(commandText, " ");
        var request = CreateRequest(commandText, "agent") with
        {
            Environment = store.CreateApprovalEnvironment(token)
        };

        Assert.True(store.TryConsumeApproval(request));
    }

    [Fact]
    public void Approval_grant_matches_normalized_environment_token()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "workspace info";
        var token = store.ApproveNextCommand(commandText, "test-agent");
        var request = CreateRequest(commandText, "test-agent") with
        {
            Environment = new Dictionary<string, string>
            {
                [MspApprovalGrantStore.DefaultEnvironmentKey] = $" {token} "
            }
        };

        Assert.True(store.TryConsumeApproval(request));
    }

    [Fact]
    public void Approval_revoke_trims_environment_token()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "workspace info";
        var token = store.ApproveNextCommand(commandText, "test-agent");
        var request = CreateRequest(commandText, "test-agent") with
        {
            Environment = store.CreateApprovalEnvironment(token)
        };

        store.RevokeApproval($" {token} ");

        Assert.False(store.TryConsumeApproval(request));
    }

    [Fact]
    public void Approval_grant_ignores_empty_environment_token_without_consuming_grant()
    {
        var store = new MspApprovalGrantStore();
        const string commandText = "workspace info";
        var token = store.ApproveNextCommand(commandText, "test-agent");
        var emptyTokenRequest = CreateRequest(commandText, "test-agent") with
        {
            Environment = new Dictionary<string, string>
            {
                [MspApprovalGrantStore.DefaultEnvironmentKey] = " "
            }
        };
        var approvedRequest = CreateRequest(commandText, "test-agent") with
        {
            Environment = store.CreateApprovalEnvironment(token)
        };

        Assert.False(store.TryConsumeApproval(emptyTokenRequest));
        Assert.True(store.TryConsumeApproval(approvedRequest));
    }

    private static MspPolicyRequest CreateRequest(string commandText, string actor)
    {
        return new MspPolicyRequest
        {
            CommandName = commandText.Split(' ', 2)[0],
            CommandText = commandText,
            Actor = actor,
            CommandMetadata = MspCommandMetadata.Create(
                commandText.Split(' ', 2)[0],
                "test command",
                effects: MspCommandEffects.WriteWorkspace)
        };
    }
}

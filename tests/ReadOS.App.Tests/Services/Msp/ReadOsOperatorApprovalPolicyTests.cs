using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsOperatorApprovalPolicyTests
{
    [Fact]
    public async Task Approval_token_allows_exact_command_and_actor_once()
    {
        var settings = new WorkspaceSettings();
        var policy = new ReadOsOperatorApprovalPolicy(() => settings);
        const string commandText = "artifact write /artifacts/notes.md \"notes\"";
        const string actor = "test-agent";
        var token = policy.ApproveNextCommand(commandText, actor);
        var environment = policy.CreateApprovalEnvironment(token);
        var request = CreateRequest(
            commandText,
            actor,
            MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact);

        var firstDecision = await policy.AuthorizeAsync(request with { Environment = environment });
        var secondDecision = await policy.AuthorizeAsync(request with { Environment = environment });

        Assert.Equal(MspPolicyDecision.Allow, firstDecision);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, secondDecision);
    }

    [Fact]
    public async Task Approval_token_is_bound_to_matching_command_and_actor_before_consumption()
    {
        var settings = new WorkspaceSettings();
        var policy = new ReadOsOperatorApprovalPolicy(() => settings);
        const string commandText = "artifact write /artifacts/notes.md \"notes\"";
        const string actor = "test-agent";
        var token = policy.ApproveNextCommand(commandText, actor);
        var environment = policy.CreateApprovalEnvironment(token);

        var mismatchedCommand = CreateRequest(
            "artifact write /artifacts/other.md \"notes\"",
            actor,
            MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact);
        var mismatchedActor = CreateRequest(
            commandText,
            "other-agent",
            MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact);
        var matchingRequest = CreateRequest(
            commandText,
            actor,
            MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact);

        var commandDecision = await policy.AuthorizeAsync(mismatchedCommand with { Environment = environment });
        var actorDecision = await policy.AuthorizeAsync(mismatchedActor with { Environment = environment });
        var matchingDecision = await policy.AuthorizeAsync(matchingRequest with { Environment = environment });

        Assert.Equal(MspPolicyDecision.RequireConfirmation, commandDecision);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, actorDecision);
        Assert.Equal(MspPolicyDecision.Allow, matchingDecision);
    }

    [Fact]
    public async Task Confirm_all_mode_requires_confirmation_for_non_dry_run_reads()
    {
        var settings = new WorkspaceSettings
        {
            MspApprovalMode = MspApprovalModeCodes.ConfirmAll
        };
        var policy = new ReadOsOperatorApprovalPolicy(() => settings);
        var request = CreateRequest("workspace info", "test-agent", MspCommandEffects.ReadWorkspace);

        var normalDecision = await policy.AuthorizeAsync(request);
        var dryRunDecision = await policy.AuthorizeAsync(request with { DryRun = true });

        Assert.Equal(MspPolicyDecision.RequireConfirmation, normalDecision);
        Assert.Equal(MspPolicyDecision.Allow, dryRunDecision);
    }

    [Theory]
    [InlineData(MspCommandEffects.WriteWorkspace, MspPolicyDecision.Allow)]
    [InlineData(MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, MspPolicyDecision.Allow)]
    [InlineData(MspCommandEffects.DeleteWorkspace, MspPolicyDecision.RequireConfirmation)]
    [InlineData(MspCommandEffects.ExternalModel, MspPolicyDecision.RequireConfirmation)]
    [InlineData(MspCommandEffects.ExternalNetwork, MspPolicyDecision.RequireConfirmation)]
    public async Task Allow_workspace_mode_allows_workspace_writes_but_requires_confirmation_for_destructive_or_external_effects(
        MspCommandEffects effects,
        MspPolicyDecision expectedDecision)
    {
        var settings = new WorkspaceSettings
        {
            MspApprovalMode = MspApprovalModeCodes.AllowWorkspace
        };
        var policy = new ReadOsOperatorApprovalPolicy(() => settings);
        var request = CreateRequest("test command", "test-agent", effects);

        var decision = await policy.AuthorizeAsync(request);

        Assert.Equal(expectedDecision, decision);
    }

    private static MspPolicyRequest CreateRequest(
        string commandText,
        string actor,
        MspCommandEffects effects)
    {
        return new MspPolicyRequest
        {
            CommandName = commandText.Split(' ', 2)[0],
            CommandText = commandText,
            Actor = actor,
            CommandMetadata = MspCommandMetadata.Create(
                commandText.Split(' ', 2)[0],
                "test command",
                effects: effects)
        };
    }
}

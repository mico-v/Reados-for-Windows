using ReadOS.Msp.Hosting.Runtime;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspCommandRequestFactoryTests
{
    [Fact]
    public void Create_applies_default_session_actor_and_working_directory()
    {
        var factory = new MspCommandRequestFactory(
            "reados-workbench",
            "reados-agent",
            "/workspace");

        var request = factory.Create("workspace info");

        Assert.Equal("workspace info", request.CommandText);
        Assert.Equal("reados-agent", request.Actor);
        Assert.Equal("reados-workbench", request.SessionId);
        Assert.Equal("/workspace", request.WorkingDirectory);
        Assert.False(request.DryRun);
        Assert.Empty(request.Environment);
    }

    [Fact]
    public void Create_allows_request_overrides()
    {
        var factory = new MspCommandRequestFactory("default-session");
        var environment = new Dictionary<string, string>
        {
            ["approval"] = "token"
        };

        var request = factory.Create(
            "artifact write /artifacts/a.md a",
            "operator",
            "session-2",
            "/documents",
            dryRun: true,
            environment);

        Assert.Equal("operator", request.Actor);
        Assert.Equal("session-2", request.SessionId);
        Assert.Equal("/documents", request.WorkingDirectory);
        Assert.True(request.DryRun);
        Assert.Same(environment, request.Environment);
    }

    [Fact]
    public void Constructor_trims_default_metadata()
    {
        var factory = new MspCommandRequestFactory(
            " reados-workbench ",
            " reados-agent ",
            " /workspace ");

        Assert.Equal("reados-workbench", factory.DefaultSessionId);
        Assert.Equal("reados-agent", factory.DefaultActor);
        Assert.Equal("/workspace", factory.DefaultWorkingDirectory);
    }

    [Fact]
    public void Constructor_uses_fallbacks_for_empty_default_metadata()
    {
        var factory = new MspCommandRequestFactory(" ", " ", " ");

        Assert.Equal("default", factory.DefaultSessionId);
        Assert.Equal("agent", factory.DefaultActor);
        Assert.Equal("/", factory.DefaultWorkingDirectory);
    }

    [Fact]
    public void Create_trims_request_metadata_overrides()
    {
        var factory = new MspCommandRequestFactory("default-session");

        var request = factory.Create(
            "workspace info",
            " operator ",
            " session-2 ",
            " /documents ");

        Assert.Equal("operator", request.Actor);
        Assert.Equal("session-2", request.SessionId);
        Assert.Equal("/documents", request.WorkingDirectory);
    }

    [Fact]
    public void Create_uses_defaults_for_empty_request_metadata_overrides()
    {
        var factory = new MspCommandRequestFactory(
            "reados-workbench",
            "reados-agent",
            "/workspace");

        var request = factory.Create(
            "workspace info",
            " ",
            " ",
            " ");

        Assert.Equal("reados-agent", request.Actor);
        Assert.Equal("reados-workbench", request.SessionId);
        Assert.Equal("/workspace", request.WorkingDirectory);
    }
}

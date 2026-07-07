using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspHostingReadinessServiceTests
{
    [Fact]
    public void BuildReport_identifies_hosted_boundaries()
    {
        var service = new ReadOsMspHostingReadinessService();

        var report = service.BuildReport();

        Assert.Equal("src/ReadOS.App/Services/Msp/ReadOsMspHost.cs", report.CurrentHost);
        Assert.Equal("src/ReadOS.Msp.Hosting", report.PlannedHost);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Session and transcript projection" &&
                boundary.TargetOwner == "ReadOS.Msp.Hosting" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Command host composition" &&
                boundary.TargetOwner == "ReadOS.Msp.Hosting" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Command host diagnostics" &&
                boundary.TargetOwner == "ReadOS.Msp.Hosting" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted);
    }

    [Fact]
    public void BuildReport_keeps_app_adapters_policy_and_runtime_core_out_of_hosting_split()
    {
        var service = new ReadOsMspHostingReadinessService();

        var report = service.BuildReport();

        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Operator policy decisions" &&
                boundary.TargetOwner == "ReadOS.App" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.AppOwned);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "ReadOS command construction" &&
                boundary.TargetOwner == "ReadOS.App" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.AppOwned);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Document, PDF, and chat adapters" &&
                boundary.TargetOwner == "ReadOS.App" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.AppOwned);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Workbench UI projection" &&
                boundary.TargetOwner == "ReadOS.App" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.AppOwned);
        Assert.Contains(
            report.Boundaries,
            boundary =>
                boundary.Name == "Parser and runtime dispatch" &&
                boundary.TargetOwner == "ReadOS.Msp" &&
                boundary.Status == ReadOsMspHostingBoundaryStatus.RuntimeOwned);
    }

    [Fact]
    public void GetHostedBoundaries_returns_only_boundaries_already_in_hosting()
    {
        var service = new ReadOsMspHostingReadinessService();

        var boundaries = service.GetHostedBoundaries();

        Assert.NotEmpty(boundaries);
        Assert.All(
            boundaries,
            boundary => Assert.Equal(ReadOsMspHostingBoundaryStatus.Hosted, boundary.Status));
        Assert.Contains(boundaries, boundary => boundary.Name == "Command host diagnostics");
        Assert.DoesNotContain(boundaries, boundary => boundary.Name == "Document, PDF, and chat adapters");
    }

    [Fact]
    public void GetReadyBoundaries_returns_only_not_yet_hosted_candidates()
    {
        var service = new ReadOsMspHostingReadinessService();

        var boundaries = service.GetReadyBoundaries();

        Assert.All(
            boundaries,
            boundary => Assert.Equal(ReadOsMspHostingBoundaryStatus.ReadyForHosting, boundary.Status));
        Assert.DoesNotContain(boundaries, boundary => boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted);
    }

    [Fact]
    public void BuildReport_keeps_command_host_diagnostics_as_internal_metadata()
    {
        var service = new ReadOsMspHostingReadinessService();

        var report = service.BuildReport();

        Assert.Equal(
            ReadOsMspCommandHostDiagnosticsVisibility.InternalHostMetadata,
            report.CommandHostDiagnosticsDecision.Visibility);
        Assert.Equal(
            "ReadOS.Msp.Hosting.Runtime.MspCommandHostDiagnostics",
            report.CommandHostDiagnosticsDecision.CurrentOwner);
        Assert.Contains("Do not add a workbench surface yet", report.CommandHostDiagnosticsDecision.WorkbenchPolicy);
        Assert.Contains(
            report.CommandHostDiagnosticsDecision.SurfaceWhen,
            condition => condition.Contains("loaded command packs"));
        Assert.Contains(
            report.CommandHostDiagnosticsDecision.SurfaceWhen,
            condition => condition.Contains("support panel"));
        Assert.Contains(
            report.CommandHostDiagnosticsDecision.SurfaceWhen,
            condition => condition.Contains("Run and Policy inspector"));
    }

    [Fact]
    public void BuildReport_lists_next_steps_in_extraction_order()
    {
        var service = new ReadOsMspHostingReadinessService();

        var report = service.BuildReport();

        Assert.Collection(
            report.NextSteps,
            step => Assert.Contains("Keep document, PDF, chat", step),
            step => Assert.Contains("Only move host-neutral contracts", step),
            step => Assert.Contains("Treat command-host diagnostics as internal host metadata", step));
    }
}

namespace ReadOS.App.Services.Msp;

internal enum ReadOsMspHostingBoundaryStatus
{
    Hosted,
    ReadyForHosting,
    NeedsHostContract,
    AppOwned,
    RuntimeOwned
}

internal enum ReadOsMspCommandHostDiagnosticsVisibility
{
    InternalHostMetadata,
    WorkbenchSurface
}

internal readonly record struct ReadOsMspHostingBoundary(
    string Name,
    string CurrentOwner,
    string TargetOwner,
    ReadOsMspHostingBoundaryStatus Status,
    string Rationale);

internal readonly record struct ReadOsMspCommandHostDiagnosticsDecision(
    ReadOsMspCommandHostDiagnosticsVisibility Visibility,
    string CurrentOwner,
    string WorkbenchPolicy,
    string Rationale,
    IReadOnlyList<string> SurfaceWhen);

internal readonly record struct ReadOsMspHostingReadinessReport(
    string CurrentHost,
    string PlannedHost,
    IReadOnlyList<ReadOsMspHostingBoundary> Boundaries,
    ReadOsMspCommandHostDiagnosticsDecision CommandHostDiagnosticsDecision,
    IReadOnlyList<string> NextSteps);

internal sealed class ReadOsMspHostingReadinessService
{
    public ReadOsMspHostingReadinessReport BuildReport()
    {
        return new ReadOsMspHostingReadinessReport(
            "src/ReadOS.App/Services/Msp/ReadOsMspHost.cs",
            "src/ReadOS.Msp.Hosting",
            new[]
            {
                new ReadOsMspHostingBoundary(
                    "Session and transcript projection",
                    "ReadOS.Msp.Hosting plus ReadOS.App workspace adapter",
                    "ReadOS.Msp.Hosting",
                    ReadOsMspHostingBoundaryStatus.Hosted,
                    "Host-neutral session projection, virtual workspace paths, and projection-store contracts live in Hosting while App persists workspace state."),
                new ReadOsMspHostingBoundary(
                    "Approval grants and approved execution",
                    "ReadOS.Msp.Hosting",
                    "ReadOS.Msp.Hosting",
                    ReadOsMspHostingBoundaryStatus.Hosted,
                    "Approval grant storage and approved command orchestration are host-neutral and covered by Hosting tests."),
                new ReadOsMspHostingBoundary(
                    "Artifact catalog and provenance",
                    "ReadOS.Msp.Hosting",
                    "ReadOS.Msp.Hosting",
                    ReadOsMspHostingBoundaryStatus.Hosted,
                    "Catalog lookup, metadata fallback, evidence checks, and source-reference classification are host-neutral; App owns UI actions and persistence."),
                new ReadOsMspHostingBoundary(
                    "Active command cancellation",
                    "ReadOS.Msp.Hosting",
                    "ReadOS.Msp.Hosting",
                    ReadOsMspHostingBoundaryStatus.Hosted,
                    "The active command registration owns linked cancellation tokens without depending on observable UI state."),
                new ReadOsMspHostingBoundary(
                    "Command host composition",
                    "ReadOS.Msp.Hosting plus ReadOS.App command pack",
                    "ReadOS.Msp.Hosting",
                    ReadOsMspHostingBoundaryStatus.Hosted,
                    "Hosting owns request construction, registry composition, runtime host creation, string-command execution, and diagnostics; App supplies the ReadOS command pack."),
                new ReadOsMspHostingBoundary(
                    "Command host diagnostics",
                    "ReadOS.Msp.Hosting",
                    "ReadOS.Msp.Hosting",
                    ReadOsMspHostingBoundaryStatus.Hosted,
                    "Request defaults, command-pack metadata, command counts, host command names, and core overrides are exposed as host metadata."),
                new ReadOsMspHostingBoundary(
                    "Operator policy decisions",
                    "ReadOS.App.Services.Msp",
                    "ReadOS.App",
                    ReadOsMspHostingBoundaryStatus.AppOwned,
                    "Confirm-all and allow-workspace choices depend on workbench settings and operator mode state."),
                new ReadOsMspHostingBoundary(
                    "ReadOS command construction",
                    "ReadOS.App.Services.Msp",
                    "ReadOS.App",
                    ReadOsMspHostingBoundaryStatus.AppOwned,
                    "The app command pack wires document, PDF, chat, attachment, workflow, and mutation delegates before Hosting composes the registry."),
                new ReadOsMspHostingBoundary(
                    "Document, PDF, and chat adapters",
                    "ReadOS.App.Services.Msp",
                    "ReadOS.App",
                    ReadOsMspHostingBoundaryStatus.AppOwned,
                    "These commands depend on app services, selected-document state, provider settings, and workbench mutation sinks."),
                new ReadOsMspHostingBoundary(
                    "Workbench UI projection",
                    "ShellViewModel",
                    "ReadOS.App",
                    ReadOsMspHostingBoundaryStatus.AppOwned,
                    "Observable routing, selected tabs, layout state, status text, and collection mutation remain WinUI responsibilities."),
                new ReadOsMspHostingBoundary(
                    "Parser and runtime dispatch",
                    "ReadOS.Msp",
                    "ReadOS.Msp",
                    ReadOsMspHostingBoundaryStatus.RuntimeOwned,
                    "Parsing, command dispatch, runtime diagnostics, and workspace contracts already belong to the portable MSP runtime.")
            },
            new ReadOsMspCommandHostDiagnosticsDecision(
                ReadOsMspCommandHostDiagnosticsVisibility.InternalHostMetadata,
                "ReadOS.Msp.Hosting.Runtime.MspCommandHostDiagnostics",
                "Do not add a workbench surface yet.",
                "The current diagnostics describe host composition and request defaults, which are useful for tests and support logs but not yet actionable operator workflow state.",
                new[]
                {
                    "Expose diagnostics in the workbench if operators need to inspect loaded command packs or overridden core commands.",
                    "Expose diagnostics in a support panel if startup validation or plugin loading begins reporting recoverable host issues.",
                    "Keep transcript/runtime command failures in the existing Run and Policy inspector surfaces."
                }),
            new[]
            {
                "Keep document, PDF, chat, and observable UI projection in ReadOS.App while Hosting grows.",
                "Only move host-neutral contracts or services that can be tested without ReadOS app models.",
                "Treat command-host diagnostics as internal host metadata until they become actionable workbench state."
            });
    }

    public IReadOnlyList<ReadOsMspHostingBoundary> GetHostedBoundaries()
    {
        return BuildReport()
            .Boundaries
            .Where(boundary => boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted)
            .ToArray();
    }

    public IReadOnlyList<ReadOsMspHostingBoundary> GetReadyBoundaries()
    {
        return BuildReport()
            .Boundaries
            .Where(boundary => boundary.Status == ReadOsMspHostingBoundaryStatus.ReadyForHosting)
            .ToArray();
    }
}

using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsFlagshipWorkflowIntegrationTests
{
    private const string Actor = "integration-agent";
    private const string EvidencePath = "/artifacts/workflows/evidence.json";
    private const string SynthesisPath = "/artifacts/workflows/evidence-synthesis.md";
    private const string RejectedRefinementPath = "/artifacts/workflows/rejected-refinement.md";

    [Fact]
    public async Task Flagship_workflow_survives_restart_and_traces_synthesis_back_to_imported_pdf_page()
    {
        using var directory = new TemporaryDirectory();
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        var fixturePath = Path.Combine(directory.Path, "service-guide.pdf");
        await File.WriteAllBytesAsync(fixturePath, "%PDF-1.7 controlled ReadOS integration fixture"u8.ToArray());

        var pdfService = new ControlledPdfDocumentService();
        var credentialStore = new InMemoryProviderCredentialStore();
        var chatService = new ControlledAiChatService("The service layer is isolated behind a durable MSP boundary.");
        var initialStore = new WorkspaceStore(pdfService, credentialStore, workspaceRoot);
        var initialWorkspace = await initialStore.LoadAsync();
        var project = Assert.Single(initialWorkspace.Projects);
        var document = await initialStore.ImportDocumentAsync(initialWorkspace, project, fixturePath);
        var documentId = document.Id;
        var importedDocumentPath = initialStore.GetAbsolutePath(document);

        Assert.True(File.Exists(importedDocumentPath));
        Assert.Equal(8, document.PageCount);
        Assert.Contains(document.Outline, item => item.Id == "section-3-2" && item.Page == 4);

        var initialHarness = new PersistentMspHarness(
            initialStore,
            pdfService,
            chatService,
            initialWorkspace,
            document);
        var extractCommand = $"workflow run extract-evidence --document current --outline \"3.2 Service Layer\" --artifact {EvidencePath}";

        var pendingExtraction = await initialHarness.RequestApprovalAsync(extractCommand);

        Assert.Equal("RequireConfirmation", pendingExtraction.Decision);
        Assert.Empty(initialWorkspace.Artifacts);
        Assert.Empty(chatService.Calls);
        Assert.Single(initialWorkspace.MspTranscript);

        var afterApprovalRestart = await RestartAsync(
            workspaceRoot,
            pdfService,
            credentialStore,
            chatService,
            documentId);
        var restoredApproval = Assert.Single(
            afterApprovalRestart.Transcripts,
            entry => entry.CommandText == extractCommand);
        Assert.True(restoredApproval.IsApprovalRequired);

        var extraction = await afterApprovalRestart.Harness.ApproveAsync(restoredApproval);

        Assert.True(extraction.Succeeded, extraction.Stderr);
        var extractedArtifact = Assert.Single(
            afterApprovalRestart.Workspace.Artifacts,
            artifact => artifact.Path == EvidencePath);
        Assert.Contains("page four evidence", extractedArtifact.Content);
        Assert.Contains("page six evidence", extractedArtifact.Content);
        Assert.Equal($"{documentId}:4-6", Assert.Single(extractedArtifact.SourcePages));
        Assert.Equal(
            new[] { 4, 5, 6 },
            extractedArtifact.SourcePaths
                .Select(path => int.Parse(Path.GetFileNameWithoutExtension(path)))
                .ToArray());

        var synthesizeCommand = $"workflow run synthesize-evidence --evidence {EvidencePath} --artifact {SynthesisPath}";
        var pendingSynthesis = await afterApprovalRestart.Harness.RequestApprovalAsync(synthesizeCommand);

        Assert.Equal("RequireConfirmation", pendingSynthesis.Decision);
        Assert.DoesNotContain(
            afterApprovalRestart.Workspace.Artifacts,
            artifact => artifact.Path == SynthesisPath);
        Assert.Empty(chatService.Calls);

        var synthesis = await afterApprovalRestart.Harness.ApproveAsync(pendingSynthesis);

        Assert.True(synthesis.Succeeded, synthesis.Stderr);
        var modelCall = Assert.Single(chatService.Calls);
        Assert.Contains("Synthesize the structured ReadOS evidence", modelCall.Prompt);
        var evidenceAttachment = Assert.Single(modelCall.Attachments);
        Assert.Equal(EvidencePath, evidenceAttachment.FilePath);
        var synthesisArtifact = Assert.Single(
            afterApprovalRestart.Workspace.Artifacts,
            artifact => artifact.Path == SynthesisPath);
        Assert.Contains("durable MSP boundary", synthesisArtifact.Content);
        Assert.Contains(EvidencePath, synthesisArtifact.SourcePaths);
        Assert.Contains(EvidencePath + ".manifest.json", synthesisArtifact.SourcePaths);
        Assert.Contains($"/documents/{documentId}/pages/5.txt", synthesisArtifact.SourcePaths);

        var refineCommand = $"workflow run refine-artifact --source {SynthesisPath} --instruction \"remove all citations\" --artifact {RejectedRefinementPath}";
        var pendingRefinement = await afterApprovalRestart.Harness.RequestApprovalAsync(refineCommand);
        var deniedRefinement = await afterApprovalRestart.Harness.DenyAsync(pendingRefinement);

        Assert.Equal(126, deniedRefinement.ExitCode);
        Assert.Equal("Deny", deniedRefinement.Decision);
        Assert.False(deniedRefinement.IsRunning);
        Assert.False(deniedRefinement.WasCanceled);
        Assert.Contains("msp.policy.denied", deniedRefinement.DiagnosticsSummary);
        Assert.Equal("Revise the command before retrying.", deniedRefinement.RecoveryHint);
        Assert.DoesNotContain(
            afterApprovalRestart.Workspace.Artifacts,
            artifact => artifact.Path == RejectedRefinementPath);
        Assert.Single(chatService.Calls);

        var recovered = await RestartAsync(
            workspaceRoot,
            pdfService,
            credentialStore,
            chatService,
            documentId);

        Assert.Equal(2, recovered.Workspace.Artifacts.Count);
        Assert.Equal(3, recovered.Transcripts.Count);
        Assert.DoesNotContain(recovered.Transcripts, entry => entry.IsApprovalRequired || entry.IsRunning);
        Assert.Contains(recovered.Transcripts, entry => entry.CommandText == extractCommand && entry.Succeeded);
        var recoveredSynthesisTranscript = Assert.Single(
            recovered.Transcripts,
            entry => entry.CommandText == synthesizeCommand);
        Assert.True(recoveredSynthesisTranscript.Succeeded);
        var recoveredDenial = Assert.Single(
            recovered.Transcripts,
            entry => entry.CommandText == refineCommand && entry.Decision == "Deny");
        Assert.False(recoveredDenial.IsRunning);
        Assert.False(recoveredDenial.IsApprovalRequired);
        Assert.Contains("msp.policy.denied", recoveredDenial.DiagnosticsSummary);

        var session = Assert.Single(recovered.Workspace.MspSessions);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, session.Id);
        Assert.Equal(3, session.CommandCount);
        Assert.Equal(1, session.ApprovalCount);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal("Deny", session.LastDecision);
        Assert.Contains(EvidencePath, session.ArtifactPaths);
        Assert.Contains(SynthesisPath, session.ArtifactPaths);

        var recoveredVirtualWorkspace = new ReadOsVirtualWorkspace(
            recovered.Store,
            pdfService,
            () => recovered.Workspace);
        var synthesisManifest = await recoveredVirtualWorkspace.TryReadTextAsync(SynthesisPath + ".manifest.json");
        Assert.NotNull(synthesisManifest);
        Assert.Contains($"\"sessionId\": \"{ReadOsMspHost.DefaultSessionId}\"", synthesisManifest);
        Assert.Contains($"\"sourceDocuments\": [\r\n    \"{documentId}\"", synthesisManifest);
        Assert.Contains($"/documents/{documentId}/pages/5.txt", synthesisManifest);

        var sessionProjection = await recoveredVirtualWorkspace.TryReadTextAsync(
            $"/sessions/{ReadOsMspHost.DefaultSessionId}.json");
        Assert.NotNull(sessionProjection);
        Assert.Contains(SynthesisPath, sessionProjection);
        Assert.Contains(recoveredSynthesisTranscript.Id, sessionProjection);

        var transcriptProjection = await recoveredVirtualWorkspace.TryReadTextAsync(
            $"/transcripts/{recoveredSynthesisTranscript.Id}.json");
        Assert.NotNull(transcriptProjection);
        Assert.Contains(synthesizeCommand, transcriptProjection);
        Assert.Contains(SynthesisPath, transcriptProjection);

        var recoveredSynthesis = Assert.Single(
            recovered.Workspace.Artifacts,
            artifact => artifact.Path == SynthesisPath);
        var artifactService = new ReadOsArtifactService();
        var synthesisLineage = artifactService.BuildLineage(
            recoveredSynthesis,
            recovered.Workspace.Artifacts);
        var evidenceLineageItem = Assert.Single(
            synthesisLineage,
            item => item.Path == EvidencePath && item.CanOpenArtifact);
        var evidenceAction = new ReadOsArtifactLineageActionService(artifactService).Resolve(
            evidenceLineageItem,
            recovered.Workspace.Artifacts);
        var recoveredEvidence = Assert.IsType<WorkspaceArtifact>(evidenceAction.Artifact);
        var evidenceLineage = artifactService.BuildLineage(
            recoveredEvidence,
            recovered.Workspace.Artifacts);
        var sourcePage = Assert.Single(
            evidenceLineage,
            item => item.Path == $"/documents/{documentId}/pages/5.txt" && item.KindLabel == "Page");

        var sourcePageText = await recoveredVirtualWorkspace.TryReadTextAsync(sourcePage.Path);
        Assert.Equal("page five evidence", sourcePageText);
        var sourcePageNumber = int.Parse(Path.GetFileNameWithoutExtension(sourcePage.Path));
        var pageNavigation = new ReadOsPageNavigationService().Prepare(
            hasWorkspace: true,
            recovered.Document,
            sourcePageNumber);
        Assert.True(pageNavigation.ShouldNavigate);
        Assert.Equal(5, pageNavigation.TargetPage);

        var persistedWorkspaceJson = await File.ReadAllTextAsync(Path.Combine(workspaceRoot, "workspace.json"));
        Assert.Contains(SynthesisPath, persistedWorkspaceJson);
        Assert.Contains(recoveredSynthesisTranscript.Id, persistedWorkspaceJson);
        Assert.Contains(ReadOsMspHost.DefaultSessionId, persistedWorkspaceJson);
    }

    [Fact]
    public async Task Canceled_extraction_persists_one_terminal_record_without_partial_artifact()
    {
        using var directory = new TemporaryDirectory();
        using var cancellation = new CancellationTokenSource();
        var pdfService = new ControlledPdfDocumentService(page =>
        {
            if (page == 5)
            {
                cancellation.Cancel();
            }
        });
        var credentialStore = new InMemoryProviderCredentialStore();
        var chatService = new ControlledAiChatService("unused");
        var scenario = await CreateScenarioAsync(directory, pdfService, credentialStore, chatService);
        var command = $"workflow run extract-evidence --document current --outline \"3.2 Service Layer\" --artifact {EvidencePath}";
        var pending = await scenario.Harness.RequestApprovalAsync(command);

        var canceledEntry = await scenario.Harness.ApproveAsync(pending, cancellation.Token);
        var canceledResult = Assert.IsType<MspCommandResult>(scenario.Harness.LastResult);

        Assert.Equal(130, canceledEntry.ExitCode);
        Assert.True(canceledEntry.WasCanceled);
        Assert.False(canceledEntry.IsRunning);
        Assert.Equal("Canceled", canceledEntry.Decision);
        Assert.Equal("MSP 命令已取消。", canceledEntry.ProgressMessage);
        Assert.Contains("msp.canceled", canceledEntry.DiagnosticsSummary);
        Assert.Empty(scenario.Workspace.Artifacts);
        Assert.Equal("msp.canceled", Assert.Single(canceledResult.Diagnostics).Code);
        var canceledAudit = Assert.Single(canceledResult.AuditRecords);
        Assert.Equal(130, canceledAudit.ExitCode);
        Assert.Equal("msp.canceled", Assert.Single(canceledAudit.Diagnostics).Code);

        var recovered = await RestartAsync(
            scenario.WorkspaceRoot,
            pdfService,
            credentialStore,
            chatService,
            scenario.Document.Id);

        var persistedCanceled = Assert.Single(recovered.Transcripts);
        Assert.Equal(canceledEntry.Id, persistedCanceled.Id);
        Assert.True(persistedCanceled.WasCanceled);
        Assert.False(persistedCanceled.IsRunning);
        Assert.False(persistedCanceled.IsApprovalRequired);
        Assert.Equal(130, persistedCanceled.ExitCode);
        Assert.Equal("Canceled", persistedCanceled.Decision);
        Assert.Contains("msp.canceled", persistedCanceled.DiagnosticsSummary);
        Assert.Empty(recovered.Workspace.Artifacts);
        var session = Assert.Single(recovered.Workspace.MspSessions);
        Assert.Equal(1, session.CommandCount);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal(0, session.RunningCount);
        Assert.Equal(0, session.PendingApprovalCount);
        Assert.Equal(130, session.LastExitCode);
        Assert.Equal("Canceled", session.LastDecision);
        Assert.Equal(persistedCanceled.Id, Assert.Single(session.TranscriptIds));
        var virtualWorkspace = new ReadOsVirtualWorkspace(
            recovered.Store,
            pdfService,
            () => recovered.Workspace);
        Assert.Null(await virtualWorkspace.TryReadTextAsync(EvidencePath));
        Assert.Null(await virtualWorkspace.TryReadTextAsync(EvidencePath + ".manifest.json"));
    }

    [Fact]
    public async Task Invalid_outline_page_returns_stable_diagnostic_without_artifact()
    {
        using var directory = new TemporaryDirectory();
        var pdfService = new ControlledPdfDocumentService();
        var credentialStore = new InMemoryProviderCredentialStore();
        var chatService = new ControlledAiChatService("unused");
        var scenario = await CreateScenarioAsync(directory, pdfService, credentialStore, chatService);
        var invalidArtifactPath = "/artifacts/workflows/invalid-range.json";
        var command = $"workflow run extract-evidence --document current --outline \"9.9 Invalid Range\" --artifact {invalidArtifactPath}";
        var pending = await scenario.Harness.RequestApprovalAsync(command);

        var failedEntry = await scenario.Harness.ApproveAsync(pending);
        var failedResult = Assert.IsType<MspCommandResult>(scenario.Harness.LastResult);

        Assert.Equal(2, failedEntry.ExitCode);
        Assert.False(failedEntry.IsRunning);
        Assert.False(failedEntry.IsApprovalRequired);
        Assert.Contains("reados.pdf.invalid_page", failedEntry.DiagnosticsSummary);
        Assert.Contains("pdf inspect current", failedEntry.RecoveryHint);
        Assert.Empty(scenario.Workspace.Artifacts);
        var diagnostic = Assert.Single(failedResult.Diagnostics);
        Assert.Equal("reados.pdf.invalid_page", diagnostic.Code);
        Assert.Equal($"{scenario.Document.Id}:9", diagnostic.Target);
        var auditDiagnostic = Assert.Single(Assert.Single(failedResult.AuditRecords).Diagnostics);
        Assert.Equal("reados.pdf.invalid_page", auditDiagnostic.Code);
        Assert.Equal(diagnostic.Target, auditDiagnostic.Target);

        var recovered = await RestartAsync(
            scenario.WorkspaceRoot,
            pdfService,
            credentialStore,
            chatService,
            scenario.Document.Id);

        var persistedFailure = Assert.Single(recovered.Transcripts);
        Assert.Equal(failedEntry.Id, persistedFailure.Id);
        Assert.Equal(2, persistedFailure.ExitCode);
        Assert.False(persistedFailure.IsRunning);
        Assert.Contains("reados.pdf.invalid_page", persistedFailure.DiagnosticsSummary);
        Assert.DoesNotContain(recovered.Workspace.Artifacts, artifact => artifact.Path == invalidArtifactPath);
        var session = Assert.Single(recovered.Workspace.MspSessions);
        Assert.Equal(1, session.CommandCount);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal(0, session.RunningCount);
        Assert.Equal(2, session.LastExitCode);
    }

    [Fact]
    public async Task Provider_failure_is_secret_safe_and_retry_after_restart_preserves_lineage_and_audit()
    {
        const string apiKey = "sk-reados-integration-secret";
        const string sensitiveRequest = "confidential-customer-request";
        using var directory = new TemporaryDirectory();
        var pdfService = new ControlledPdfDocumentService();
        var credentialStore = new InMemoryProviderCredentialStore();
        var failingChatService = new ControlledAiChatService(
            new AiChatServiceException($"provider echoed {apiKey} and {sensitiveRequest}"));
        var scenario = await CreateScenarioAsync(directory, pdfService, credentialStore, failingChatService);
        scenario.Workspace.Settings.ProviderName = "Integration Provider";
        scenario.Workspace.Settings.ProviderBaseUrl = "https://provider.example/v1";
        scenario.Workspace.Settings.ProviderApiKey = apiKey;
        scenario.Workspace.Settings.ModelName = "integration-model";
        scenario.Workspace.Settings.UseOfflineResponses = false;
        await scenario.Store.SaveAsync(scenario.Workspace);

        var extractCommand = $"workflow run extract-evidence --document current --outline \"3.2 Service Layer\" --artifact {EvidencePath}";
        var pendingExtraction = await scenario.Harness.RequestApprovalAsync(extractCommand);
        var extraction = await scenario.Harness.ApproveAsync(pendingExtraction);
        Assert.True(extraction.Succeeded, extraction.Stderr);
        Assert.Single(scenario.Workspace.Artifacts, artifact => artifact.Path == EvidencePath);

        var synthesizeCommand = $"workflow run synthesize-evidence --evidence {EvidencePath} --artifact {SynthesisPath}";
        var pendingSynthesis = await scenario.Harness.RequestApprovalAsync(synthesizeCommand);
        var providerFailure = await scenario.Harness.ApproveAsync(pendingSynthesis);
        var providerFailureResult = Assert.IsType<MspCommandResult>(scenario.Harness.LastResult);

        Assert.Equal(1, providerFailure.ExitCode);
        Assert.False(providerFailure.IsRunning);
        Assert.Contains("reados.chat.model_provider_failed", providerFailure.DiagnosticsSummary);
        Assert.DoesNotContain(scenario.Workspace.Artifacts, artifact => artifact.Path == SynthesisPath);
        Assert.Single(failingChatService.Calls);
        AssertSensitiveValuesAbsent(providerFailure, providerFailureResult, apiKey, sensitiveRequest);
        var providerFailureAudit = Assert.Single(providerFailureResult.AuditRecords);
        Assert.Equal("reados.chat.model_provider_failed", Assert.Single(providerFailureAudit.Diagnostics).Code);
        var persistedWorkspace = await File.ReadAllTextAsync(Path.Combine(scenario.WorkspaceRoot, "workspace.json"));
        Assert.DoesNotContain(apiKey, persistedWorkspace, StringComparison.Ordinal);
        Assert.DoesNotContain(sensitiveRequest, persistedWorkspace, StringComparison.Ordinal);

        var successfulChatService = new ControlledAiChatService(
            "Recovered synthesis with durable source lineage.");
        var afterFailureRestart = await RestartAsync(
            scenario.WorkspaceRoot,
            pdfService,
            credentialStore,
            successfulChatService,
            scenario.Document.Id);

        Assert.Equal(apiKey, afterFailureRestart.Workspace.Settings.ProviderApiKey);
        var restoredFailure = Assert.Single(
            afterFailureRestart.Transcripts,
            entry => entry.CommandText == synthesizeCommand);
        Assert.Equal(providerFailure.Id, restoredFailure.Id);
        Assert.Equal(1, restoredFailure.ExitCode);
        Assert.Contains("reados.chat.model_provider_failed", restoredFailure.DiagnosticsSummary);
        AssertSensitiveValuesAbsent(restoredFailure, providerFailureResult, apiKey, sensitiveRequest);

        var retryApproval = await afterFailureRestart.Harness.RequestApprovalAsync(synthesizeCommand);
        var successfulRetry = await afterFailureRestart.Harness.ApproveAsync(retryApproval);
        var successfulRetryResult = Assert.IsType<MspCommandResult>(afterFailureRestart.Harness.LastResult);

        Assert.True(successfulRetry.Succeeded, successfulRetry.Stderr);
        Assert.Equal("Allow", successfulRetry.Decision);
        var retryArtifact = Assert.Single(
            afterFailureRestart.Workspace.Artifacts,
            artifact => artifact.Path == SynthesisPath);
        Assert.Contains(EvidencePath, retryArtifact.SourcePaths);
        Assert.Contains(EvidencePath + ".manifest.json", retryArtifact.SourcePaths);
        Assert.Contains($"/documents/{scenario.Document.Id}/pages/5.txt", retryArtifact.SourcePaths);
        var retryAudit = Assert.Single(successfulRetryResult.AuditRecords);
        Assert.Equal(0, retryAudit.ExitCode);
        Assert.Equal("Allow", retryAudit.Decision.ToString());
        Assert.Equal(synthesizeCommand, retryAudit.CommandText);

        var recovered = await RestartAsync(
            scenario.WorkspaceRoot,
            pdfService,
            credentialStore,
            successfulChatService,
            scenario.Document.Id);

        Assert.Equal(2, recovered.Workspace.Artifacts.Count);
        var synthesisTranscripts = recovered.Transcripts
            .Where(entry => entry.CommandText == synthesizeCommand)
            .OrderBy(entry => entry.StartedAt)
            .ToArray();
        Assert.Equal(2, synthesisTranscripts.Length);
        Assert.Equal(1, synthesisTranscripts[0].ExitCode);
        Assert.Equal(0, synthesisTranscripts[1].ExitCode);
        Assert.All(synthesisTranscripts, entry => Assert.False(entry.IsRunning));
        var session = Assert.Single(recovered.Workspace.MspSessions);
        Assert.Equal(3, session.CommandCount);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal(0, session.RunningCount);
        Assert.Equal(0, session.PendingApprovalCount);
        Assert.Equal(0, session.LastExitCode);
        Assert.Contains(EvidencePath, session.ArtifactPaths);
        Assert.Contains(SynthesisPath, session.ArtifactPaths);

        var recoveredSynthesis = Assert.Single(
            recovered.Workspace.Artifacts,
            artifact => artifact.Path == SynthesisPath);
        var artifactService = new ReadOsArtifactService();
        var synthesisLineage = artifactService.BuildLineage(
            recoveredSynthesis,
            recovered.Workspace.Artifacts);
        var evidenceLineage = Assert.Single(
            synthesisLineage,
            item => item.Path == EvidencePath && item.CanOpenArtifact);
        var evidenceArtifact = Assert.IsType<WorkspaceArtifact>(
            new ReadOsArtifactLineageActionService(artifactService)
                .Resolve(evidenceLineage, recovered.Workspace.Artifacts)
                .Artifact);
        var sourcePage = Assert.Single(
            artifactService.BuildLineage(evidenceArtifact, recovered.Workspace.Artifacts),
            item => item.Path == $"/documents/{scenario.Document.Id}/pages/5.txt");
        var virtualWorkspace = new ReadOsVirtualWorkspace(
            recovered.Store,
            pdfService,
            () => recovered.Workspace);
        Assert.Equal("page five evidence", await virtualWorkspace.TryReadTextAsync(sourcePage.Path));
    }

    private static async Task<ImportedScenario> CreateScenarioAsync(
        TemporaryDirectory directory,
        IPdfDocumentService pdfService,
        IProviderCredentialStore credentialStore,
        IAiChatService chatService)
    {
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        var fixturePath = Path.Combine(directory.Path, "service-guide.pdf");
        await File.WriteAllBytesAsync(
            fixturePath,
            "%PDF-1.7 controlled ReadOS integration fixture"u8.ToArray());
        var store = new WorkspaceStore(pdfService, credentialStore, workspaceRoot);
        var workspace = await store.LoadAsync();
        var project = Assert.Single(workspace.Projects);
        var document = await store.ImportDocumentAsync(workspace, project, fixturePath);
        var harness = new PersistentMspHarness(
            store,
            pdfService,
            chatService,
            workspace,
            document);
        return new ImportedScenario(
            workspaceRoot,
            store,
            workspace,
            document,
            harness);
    }

    private static void AssertSensitiveValuesAbsent(
        MspTranscriptEntry entry,
        MspCommandResult result,
        params string[] sensitiveValues)
    {
        var resultText = string.Join(
            Environment.NewLine,
            new[]
            {
                result.Stdout,
                result.Stderr,
                entry.Stdout,
                entry.Stderr,
                entry.DiagnosticsSummary,
                entry.RecoveryHint,
                entry.PolicyPreview,
                entry.ProgressMessage
            }
            .Concat(result.Diagnostics.Select(diagnostic => diagnostic.ToDisplayText()))
            .Concat(result.AuditRecords.SelectMany(audit => new[]
            {
                audit.Message ?? string.Empty,
                audit.Preview.ToDisplayText()
            }))
            .Concat(result.AuditRecords.SelectMany(audit =>
                audit.Diagnostics.Select(diagnostic => diagnostic.ToDisplayText()))));

        foreach (var sensitiveValue in sensitiveValues)
        {
            Assert.DoesNotContain(sensitiveValue, resultText, StringComparison.Ordinal);
        }
    }

    private static async Task<RestartedHarness> RestartAsync(
        string workspaceRoot,
        IPdfDocumentService pdfService,
        IProviderCredentialStore credentialStore,
        IAiChatService chatService,
        string documentId)
    {
        var store = new WorkspaceStore(pdfService, credentialStore, workspaceRoot);
        var workspace = await store.LoadAsync();
        var document = workspace.Projects
            .SelectMany(project => project.LibraryItems)
            .Single(item => item.Id == documentId);
        var harness = new PersistentMspHarness(
            store,
            pdfService,
            chatService,
            workspace,
            document);
        var transcripts = harness.RefreshTranscripts();
        await store.SaveAsync(workspace);
        return new RestartedHarness(store, workspace, document, harness, transcripts);
    }

    private sealed record ImportedScenario(
        string WorkspaceRoot,
        WorkspaceStore Store,
        WorkspaceState Workspace,
        LibraryItem Document,
        PersistentMspHarness Harness);

    private sealed record RestartedHarness(
        WorkspaceStore Store,
        WorkspaceState Workspace,
        LibraryItem Document,
        PersistentMspHarness Harness,
        IReadOnlyList<MspTranscriptEntry> Transcripts);

    private sealed class PersistentMspHarness
    {
        private readonly WorkspaceStore store;
        private readonly WorkspaceState workspace;
        private readonly ReadOsMspHost host;
        private readonly ReadOsMspCommandTranscriptService transcriptService = new(ReadOsMspHost.DefaultSessionId);
        private readonly ReadOsMspTranscriptWorkspaceService transcriptWorkspaceService = new(
            ReadOsMspHost.DefaultSessionId,
            maxTranscriptEntries: 50);
        private readonly ReadOsMspApprovalReviewService approvalReviewService = new();
        private readonly DateTimeOffset startedAt;
        private int commandIndex;

        public PersistentMspHarness(
            WorkspaceStore store,
            IPdfDocumentService pdfService,
            IAiChatService chatService,
            WorkspaceState workspace,
            LibraryItem document)
        {
            this.store = store;
            this.workspace = workspace;
            startedAt = workspace.MspTranscript
                .Select(entry => entry.CompletedAt)
                .DefaultIfEmpty(new DateTimeOffset(2026, 7, 10, 8, 0, 0, TimeSpan.Zero))
                .Max();
            host = new ReadOsMspHost(new ReadOsMspHostDependencies(
                store,
                pdfService,
                chatService,
                () => workspace,
                () => workspace.Settings,
                () => document,
                () => Array.Empty<ChatAttachment>(),
                _ => Task.FromResult(string.Empty),
                _ => { },
                () => { },
                (_, _) => { }));
        }

        public MspCommandResult? LastResult { get; private set; }

        public IReadOnlyList<MspTranscriptEntry> RefreshTranscripts()
        {
            return transcriptWorkspaceService.RefreshTranscript(workspace);
        }

        public async Task<MspTranscriptEntry> RequestApprovalAsync(string commandText)
        {
            return await ExecuteAndPersistAsync(commandText, approved: false);
        }

        public async Task<MspTranscriptEntry> ApproveAsync(
            MspTranscriptEntry approvalEntry,
            CancellationToken cancellationToken = default)
        {
            Assert.True(approvalReviewService.CanReview(approvalEntry));
            Assert.True(transcriptWorkspaceService.RemoveTranscriptEntry(workspace, approvalEntry));
            await store.SaveAsync(workspace);
            return await ExecuteAndPersistAsync(
                approvalEntry.CommandText,
                approved: true,
                cancellationToken);
        }

        public async Task<MspTranscriptEntry> DenyAsync(MspTranscriptEntry approvalEntry)
        {
            Assert.True(approvalReviewService.CanReview(approvalEntry));
            Assert.True(transcriptWorkspaceService.RemoveTranscriptEntry(workspace, approvalEntry));
            var deniedEntry = approvalReviewService.CreateDeniedEntry(
                approvalEntry,
                NextStartedAt().AddSeconds(1));
            Assert.True(transcriptWorkspaceService.PersistTranscriptEntry(workspace, deniedEntry, 0));
            await store.SaveAsync(workspace);
            return deniedEntry;
        }

        private async Task<MspTranscriptEntry> ExecuteAndPersistAsync(
            string commandText,
            bool approved,
            CancellationToken cancellationToken = default)
        {
            var entry = transcriptService.CreateRunningEntry(commandText, Actor, NextStartedAt());
            var result = approved
                ? await host.ExecuteApprovedAsync(commandText, Actor, cancellationToken)
                : await host.ExecuteAsync(commandText, Actor, cancellationToken);
            LastResult = result;
            transcriptService.CompleteEntry(entry, result, entry.StartedAt.AddSeconds(1));
            Assert.True(transcriptWorkspaceService.PersistTranscriptEntry(workspace, entry, 0));
            await store.SaveAsync(workspace);
            return entry;
        }

        private DateTimeOffset NextStartedAt()
        {
            commandIndex++;
            return startedAt.AddMinutes(commandIndex);
        }
    }

    private sealed class ControlledPdfDocumentService : IPdfDocumentService
    {
        private static readonly IReadOnlyDictionary<int, string> PageText = new Dictionary<int, string>
        {
            [4] = "page four evidence",
            [5] = "page five evidence",
            [6] = "page six evidence"
        };

        private readonly Action<int>? beforeExtractPage;

        public ControlledPdfDocumentService(Action<int>? beforeExtractPage = null)
        {
            this.beforeExtractPage = beforeExtractPage;
        }

        public Task<PdfDocumentInfo> InspectAsync(
            string path,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Assert.True(File.Exists(path));
            return Task.FromResult(new PdfDocumentInfo(
                8,
                new[]
                {
                    new PageLabelRule { PdfPage = 4, Label = "S-1" }
                },
                new[]
                {
                    new OutlineItem { Id = "chapter-3", Title = "3 Architecture", Page = 2, Level = 1 },
                    new OutlineItem { Id = "section-3-2", Title = "3.2 Service Layer", Page = 4, Level = 2 },
                    new OutlineItem { Id = "section-3-3", Title = "3.3 Agent Bridge", Page = 7, Level = 2 },
                    new OutlineItem { Id = "invalid-range", Title = "9.9 Invalid Range", Page = 9, Level = 2 }
                }));
        }

        public Task<BitmapImage> RenderPageAsync(
            string path,
            int pageNumber,
            double width,
            CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(
            string path,
            int pageCount,
            int maxPages,
            CancellationToken cancellationToken = default)
        {
            return Task.FromResult<IReadOnlyList<PageImageItem>>(Array.Empty<PageImageItem>());
        }

        public Task<IReadOnlyList<PdfTextHit>> SearchAsync(
            string path,
            string query,
            CancellationToken cancellationToken = default)
        {
            return Task.FromResult<IReadOnlyList<PdfTextHit>>(Array.Empty<PdfTextHit>());
        }

        public Task<string> ExtractPageTextAsync(
            string path,
            int startPage,
            int endPage,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Assert.True(File.Exists(path));
            Assert.Equal(startPage, endPage);
            beforeExtractPage?.Invoke(startPage);
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(PageText.TryGetValue(startPage, out var text) ? text : string.Empty);
        }
    }

    private sealed class ControlledAiChatService : IAiChatService
    {
        private readonly string? response;
        private readonly Exception? failure;

        public ControlledAiChatService(string response)
        {
            this.response = response;
        }

        public ControlledAiChatService(Exception failure)
        {
            this.failure = failure;
        }

        public List<AiCall> Calls { get; } = new();

        public Task<string> SendAsync(
            WorkspaceSettings settings,
            LibraryItem? document,
            IEnumerable<ChatMessage> history,
            string userPrompt,
            IEnumerable<ChatAttachment> attachments,
            Func<ChatAttachment, Task<string>> attachmentTextProvider,
            string? mspInstruction = null,
            string? mspExecutionContext = null,
            bool allowMspCommandRequests = true,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Calls.Add(new AiCall(userPrompt, attachments.ToArray()));
            return failure is null
                ? Task.FromResult(response ?? string.Empty)
                : Task.FromException<string>(failure);
        }
    }

    private sealed record AiCall(string Prompt, IReadOnlyList<ChatAttachment> Attachments);

    private sealed class InMemoryProviderCredentialStore : IProviderCredentialStore
    {
        private readonly Dictionary<string, string> credentials = new(StringComparer.Ordinal);

        public Task<string?> GetApiKeyAsync(
            string providerBaseUrl,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            credentials.TryGetValue(providerBaseUrl, out var apiKey);
            return Task.FromResult<string?>(apiKey);
        }

        public Task SetApiKeyAsync(
            string providerBaseUrl,
            string? apiKey,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (string.IsNullOrWhiteSpace(apiKey))
            {
                credentials.Remove(providerBaseUrl);
            }
            else
            {
                credentials[providerBaseUrl] = apiKey.Trim();
            }

            return Task.CompletedTask;
        }
    }

    private sealed class TemporaryDirectory : IDisposable
    {
        public TemporaryDirectory()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                "ReadOS.Tests",
                Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(Path);
        }

        public string Path { get; }

        public void Dispose()
        {
            if (Directory.Exists(Path))
            {
                Directory.Delete(Path, recursive: true);
            }
        }
    }
}

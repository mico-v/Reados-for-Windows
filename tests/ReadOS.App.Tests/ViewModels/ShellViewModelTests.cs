using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;
using ReadOS.Msp.Models;

namespace ReadOS.App.Tests.ViewModels;

public sealed class ShellViewModelTests
{
    [Fact]
    public void Thread_timeline_items_project_typed_msp_record_states()
    {
        var approval = ThreadTimelineItem.FromMsp(new MspTranscriptEntry
        {
            Actor = "agent",
            CommandText = "artifact write /artifacts/pending.md pending",
            Decision = "RequireConfirmation",
            PolicyPreview = "Create artifact /artifacts/pending.md"
        });
        var running = ThreadTimelineItem.FromMsp(new MspTranscriptEntry
        {
            Actor = "agent",
            CommandText = "pdf search current alpha",
            IsRunning = true,
            ProgressPercent = 40,
            ProgressMessage = "Searching PDF"
        });
        var failed = ThreadTimelineItem.FromMsp(new MspTranscriptEntry
        {
            Actor = "agent",
            CommandText = "pdf inspect missing",
            ExitCode = 1,
            DiagnosticsSummary = "error reados.pdf.document_not_found: PDF document not found"
        });
        var completed = ThreadTimelineItem.FromMsp(new MspTranscriptEntry
        {
            Actor = "agent",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "documents\t1"
        });
        var artifact = ThreadTimelineItem.FromArtifact(new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            MediaType = "text/markdown",
            Preview = "summary"
        });
        var evidence = ThreadTimelineItem.FromMessage(new ChatMessage
        {
            Role = ChatRole.System,
            Author = "Evidence",
            Content = "attached page text"
        });

        Assert.Equal(TimelineItemKind.Approval, approval.Kind);
        Assert.Equal("Approval", approval.KindLabel);
        Assert.Equal("MSP approval required", approval.Title);
        Assert.Contains("artifact write", approval.Subtitle);
        Assert.True(approval.IsMspApproval);
        Assert.False(approval.IsMspResult);

        Assert.Equal(TimelineItemKind.MspCommand, running.Kind);
        Assert.Equal("MSP", running.KindLabel);
        Assert.Equal("MSP command running", running.Title);
        Assert.Equal("运行 40%", running.StatusLabel);
        Assert.True(running.IsMspRunning);
        Assert.False(running.IsMspResult);

        Assert.Equal(TimelineItemKind.Error, failed.Kind);
        Assert.Equal("Error", failed.KindLabel);
        Assert.Equal("MSP command failed", failed.Title);
        Assert.Contains("reados.pdf.document_not_found", failed.Body);
        Assert.True(failed.IsMspError);
        Assert.False(failed.IsMspResult);

        Assert.Equal(TimelineItemKind.MspCommand, completed.Kind);
        Assert.Equal("MSP command completed", completed.Title);
        Assert.True(completed.IsMspResult);

        Assert.Equal(TimelineItemKind.Artifact, artifact.Kind);
        Assert.Equal("Artifact", artifact.KindLabel);
        Assert.Equal("Artifact", artifact.Title);
        Assert.True(artifact.HasArtifact);

        Assert.Equal(TimelineItemKind.Evidence, evidence.Kind);
        Assert.Equal("Evidence", evidence.KindLabel);
        Assert.True(evidence.HasEvidenceBody);
        Assert.False(evidence.HasMessageBody);
    }

    [Fact]
    public async Task Open_timeline_item_routes_to_matching_inspector_context()
    {
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/report.md",
            Content = "artifact content",
            Description = "Report",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero)
        });
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "failed-command",
            Actor = "agent",
            SessionId = "reados-workbench",
            CommandText = "pdf inspect missing",
            ExitCode = 1,
            DiagnosticsSummary = "error reados.pdf.document_not_found: PDF document not found",
            RecoveryHint = "Run library list.",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 1, TimeSpan.Zero)
        });
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "completed-command",
            Actor = "agent",
            SessionId = "reados-workbench",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "documents\t1",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 2, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 2, 1, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.IsInspectorVisible = false;

        var artifactItem = Assert.Single(viewModel.TimelineItems, item => item.Kind == TimelineItemKind.Artifact);
        viewModel.OpenTimelineItemCommand.Execute(artifactItem);

        Assert.True(viewModel.IsInspectorVisible);
        Assert.Equal(InspectorTab.Artifacts, viewModel.SelectedInspectorTab);
        Assert.Equal("/artifacts/report.md", viewModel.SelectedArtifact?.Path);

        var errorItem = Assert.Single(viewModel.TimelineItems, item => item.Kind == TimelineItemKind.Error);
        viewModel.OpenTimelineItemCommand.Execute(errorItem);

        Assert.Equal(InspectorTab.Policy, viewModel.SelectedInspectorTab);
        Assert.Equal("failed-command", viewModel.SelectedMspTranscriptEntry?.Id);
        Assert.True(viewModel.Inspector.HasSelectedMspTranscriptEntry);
        Assert.Equal("failed-command", viewModel.Inspector.SelectedMspTranscriptEntry?.Id);
        Assert.True(viewModel.MspTranscript.Single(entry => entry.Id == "failed-command").IsInspectorSelected);
        Assert.False(viewModel.MspTranscript.Single(entry => entry.Id == "completed-command").IsInspectorSelected);

        viewModel.OpenTimelineItemCommand.Execute(ThreadTimelineItem.FromMessage(new ChatMessage
        {
            Role = ChatRole.System,
            Author = "Evidence",
            Content = "attached evidence"
        }));

        Assert.Equal(InspectorTab.Evidence, viewModel.SelectedInspectorTab);
    }

    [Fact]
    public async Task Deny_msp_command_keeps_denied_entry_selected()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "approval-command",
            Actor = "agent",
            SessionId = "reados-workbench",
            CommandText = "artifact write /artifacts/pending.md pending",
            Decision = "RequireConfirmation",
            Effects = "Write artifact",
            PolicyPreview = "Create artifact /artifacts/pending.md",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 3, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 3, 1, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        var approval = Assert.Single(viewModel.MspTranscript);
        viewModel.SelectedMspTranscriptEntry = approval;

        await viewModel.DenyMspCommandCommand.ExecuteAsync(approval);

        var denied = Assert.Single(viewModel.MspTranscript);
        Assert.Equal("Deny", denied.Decision);
        Assert.Equal(denied.Id, viewModel.SelectedMspTranscriptEntry?.Id);
        Assert.True(viewModel.Inspector.HasSelectedMspTranscriptEntry);
        Assert.True(denied.IsInspectorSelected);
        Assert.Equal(TimelineItemKind.Error, ThreadTimelineItem.FromMsp(denied).Kind);
    }

    [Fact]
    public async Task Run_msp_command_records_durable_session()
    {
        var workspace = new WorkspaceState();
        var workspaceStore = new TestWorkspaceStore(workspace);
        var viewModel = new ShellViewModel(
            workspaceStore,
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.MspCommandDraft = "workspace info";

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("reados-workbench", transcript.SessionId);
        Assert.Equal("workspace info", transcript.CommandText);
        Assert.True(transcript.Succeeded, transcript.Stderr);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("reados-workbench", session.Id);
        Assert.Equal(1, session.CommandCount);
        Assert.Equal("workspace info", session.LastCommandText);
        Assert.Equal("Allow", session.LastDecision);
        Assert.Equal(transcript.Id, Assert.Single(session.TranscriptIds));
        Assert.Empty(session.ArtifactPaths);
        Assert.True(workspaceStore.SaveCount > 0);
    }

    [Fact]
    public async Task Run_msp_command_records_policy_diagnostics_in_session()
    {
        var workspace = new WorkspaceState();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.MspCommandDraft = "artifact write /artifacts/pending.md \"pending\"";

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("RequireConfirmation", transcript.Decision);
        Assert.Contains("msp.policy.require_confirmation", transcript.DiagnosticsSummary);
        Assert.Contains("Approve", transcript.RecoveryHint);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal(1, session.PendingApprovalCount);
        Assert.True(session.HasPendingApprovals);
        Assert.Equal("待审批", session.StatusLabel);
        Assert.Contains("msp.policy.require_confirmation", session.LastDiagnosticsSummary);
        Assert.Contains("Approve", session.LastRecoveryHint);
    }

    [Fact]
    public async Task Approval_mode_controls_persist_and_forward_to_child_view_models()
    {
        var workspace = new WorkspaceState
        {
            Settings = new WorkspaceSettings
            {
                MspApprovalMode = MspApprovalModeCodes.ConfirmAll
            }
        };
        var workspaceStore = new TestWorkspaceStore(workspace);
        var viewModel = new ShellViewModel(
            workspaceStore,
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        Assert.Equal(MspApprovalModeCodes.ConfirmAll, viewModel.ApprovalModeCode);
        Assert.True(viewModel.IsConfirmAllApprovalModeSelected);
        Assert.True(viewModel.Thread.IsConfirmAllApprovalModeSelected);
        Assert.True(viewModel.Inspector.IsConfirmAllApprovalModeSelected);
        Assert.Equal("每次确认", viewModel.Thread.ApprovalModeLabel);

        var saveCount = workspaceStore.SaveCount;
        viewModel.Thread.SelectApprovalModeCommand.Execute(MspApprovalModeCodes.AllowWorkspace);

        Assert.Equal(MspApprovalModeCodes.AllowWorkspace, workspace.Settings.MspApprovalMode);
        Assert.Equal(MspApprovalModeCodes.AllowWorkspace, viewModel.ApprovalModeCode);
        Assert.True(viewModel.IsAllowWorkspaceApprovalModeSelected);
        Assert.True(viewModel.Thread.IsAllowWorkspaceApprovalModeSelected);
        Assert.True(viewModel.Inspector.IsAllowWorkspaceApprovalModeSelected);
        Assert.Contains("允许写入", viewModel.ApprovalModeLabel);
        Assert.True(workspaceStore.SaveCount > saveCount);
    }

    [Fact]
    public async Task Run_drawer_layout_persists_height_and_pin()
    {
        var workspace = new WorkspaceState
        {
            Settings = new WorkspaceSettings
            {
                RunDrawerHeight = 320,
                IsRunDrawerPinned = true
            }
        };
        var workspaceStore = new TestWorkspaceStore(workspace);
        var viewModel = new ShellViewModel(
            workspaceStore,
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        Assert.Equal(320, viewModel.RunDrawerHeight);
        Assert.True(viewModel.IsRunDrawerPinned);

        viewModel.RunDrawerHeight = 400;
        await viewModel.CommitRunDrawerLayoutAsync();

        Assert.Equal(400, workspace.Settings.RunDrawerHeight);
        Assert.True(workspaceStore.SaveCount > 0);

        viewModel.PinRunDrawerCommand.Execute(null);

        Assert.False(viewModel.IsRunDrawerPinned);
        Assert.False(workspace.Settings.IsRunDrawerPinned);
    }

    [Fact]
    public async Task Approval_mode_changes_runtime_policy_for_msp_commands()
    {
        var workspace = new WorkspaceState();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.Thread.SelectApprovalModeCommand.Execute(MspApprovalModeCodes.AllowWorkspace);
        viewModel.MspCommandDraft = "artifact write /artifacts/allowed.md \"allowed\"";

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.True(transcript.Succeeded, transcript.Stderr);
        Assert.Equal("Allow", transcript.Decision);
        Assert.Empty(transcript.DiagnosticsSummary);
        Assert.False(viewModel.HasPendingApprovals);
        Assert.Contains(workspace.Artifacts, artifact => artifact.Path == "/artifacts/allowed.md");

        viewModel.Thread.SelectApprovalModeCommand.Execute(MspApprovalModeCodes.ConfirmAll);
        viewModel.MspCommandDraft = "workspace info";

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var pending = workspace.MspTranscript.First(entry => entry.CommandText == "workspace info");
        Assert.Equal("RequireConfirmation", pending.Decision);
        Assert.Contains("msp.policy.require_confirmation", pending.DiagnosticsSummary);
        Assert.True(viewModel.HasPendingApprovals);
    }

    [Fact]
    public async Task Open_pending_approval_routes_to_policy_inspector()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "completed-command",
            SessionId = "reados-workbench",
            Actor = "agent",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "ok",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 1, TimeSpan.Zero)
        });
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "approval-command",
            SessionId = "reados-workbench",
            Actor = "agent",
            CommandText = "artifact write /artifacts/pending.md pending",
            Decision = "RequireConfirmation",
            PolicyPreview = "Create artifact /artifacts/pending.md",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 1, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.IsInspectorVisible = false;
        viewModel.IsRunDrawerOpen = false;

        viewModel.Thread.OpenPendingApprovalCommand.Execute(null);

        Assert.True(viewModel.IsInspectorVisible);
        Assert.True(viewModel.IsRunDrawerOpen);
        Assert.Equal(InspectorTab.Policy, viewModel.SelectedInspectorTab);
        Assert.Equal("approval-command", viewModel.SelectedMspTranscriptEntry?.Id);
        Assert.True(viewModel.MspTranscript.Single(entry => entry.Id == "approval-command").IsInspectorSelected);
        Assert.Contains("待审批 MSP 命令", viewModel.StatusMessage);
    }

    [Fact]
    public async Task Selecting_msp_session_routes_to_latest_relevant_transcript()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "completed-command",
            SessionId = "review-session",
            Actor = "agent",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "ok",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 1, TimeSpan.Zero)
        });
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "failed-command",
            SessionId = "review-session",
            Actor = "agent",
            CommandText = "pdf inspect missing",
            ExitCode = 1,
            DiagnosticsSummary = "error reados.pdf.document_not_found: PDF document not found",
            RecoveryHint = "Run library list.",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 1, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        var session = Assert.Single(viewModel.MspSessions);

        viewModel.SelectedMspSession = session;

        Assert.True(viewModel.IsInspectorVisible);
        Assert.Equal(InspectorTab.Policy, viewModel.SelectedInspectorTab);
        Assert.Equal("failed-command", viewModel.SelectedMspTranscriptEntry?.Id);
        Assert.True(viewModel.MspTranscript.Single(entry => entry.Id == "failed-command").IsInspectorSelected);
        Assert.Equal("需复查", session.StatusLabel);
        Assert.True(session.HasFailures);
    }

    [Fact]
    public async Task Policy_failure_review_action_prepares_review_failures_workflow()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "completed-command",
            SessionId = "review-session",
            Actor = "agent",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "ok",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 1, TimeSpan.Zero)
        });
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "failed-command",
            SessionId = "review-session",
            Actor = "agent",
            CommandText = "pdf inspect missing",
            ExitCode = 1,
            DiagnosticsSummary = "error reados.pdf.document_not_found: PDF document not found",
            RecoveryHint = "Run library list.",
            StartedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 9, 1, 1, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedMspTranscriptEntry = viewModel.MspTranscript.Single(entry => entry.Id == "completed-command");

        Assert.False(viewModel.HasSelectedFailureReviewTarget);
        Assert.False(viewModel.Inspector.HasSelectedFailureReviewTarget);

        viewModel.SelectedMspTranscriptEntry = viewModel.MspTranscript.Single(entry => entry.Id == "failed-command");

        Assert.True(viewModel.HasSelectedFailureReviewTarget);
        Assert.True(viewModel.Inspector.HasSelectedFailureReviewTarget);

        viewModel.Inspector.PrepareReviewFailuresWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run review-failures --artifact \"/artifacts/workflows/review-session-failed-command-failures.md\"",
            viewModel.MspCommandDraft);
        Assert.Equal(viewModel.MspCommandDraft, viewModel.Inspector.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);
        Assert.True(viewModel.IsInspectorVisible);
        Assert.Contains("失败复查工作流", viewModel.StatusMessage);
    }

    [Fact]
    public async Task Initialize_rebuilds_sessions_and_removes_stale_records()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "transcript-1",
            SessionId = string.Empty,
            Actor = "agent",
            CommandText = "workspace info",
            StartedAt = new DateTimeOffset(2026, 6, 30, 8, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 6, 30, 8, 0, 1, TimeSpan.Zero),
            Decision = "Allow"
        });
        workspace.MspSessions.Add(new MspSessionEntry
        {
            Id = "stale-session",
            Title = "Stale session"
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("reados-workbench", transcript.SessionId);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("reados-workbench", session.Id);
        Assert.Equal("workspace info", session.LastCommandText);
        Assert.Equal("transcript-1", Assert.Single(session.TranscriptIds));

        var visibleSession = Assert.Single(viewModel.MspSessions);
        Assert.Equal("reados-workbench", visibleSession.Id);
        Assert.Equal("workspace info", visibleSession.LastCommandText);
        Assert.Contains(viewModel.TimelineItems, item =>
            item.Kind == TimelineItemKind.MspCommand &&
            item.MspEntry?.Id == "transcript-1");
        Assert.Equal("1 条 MSP 执行记录", viewModel.MspActivitySummary);
    }

    [Fact]
    public async Task Initialize_projects_artifacts_into_sessions_and_timeline()
    {
        var updatedAt = new DateTimeOffset(2026, 7, 1, 9, 30, 0, TimeSpan.Zero);
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/brief.md",
            Content = "full artifact content",
            Preview = "artifact preview",
            Description = "Investigation brief",
            MediaType = "text/markdown",
            SourceCommand = "artifact write /artifacts/brief.md",
            SessionId = "artifact-session",
            CreatedAt = updatedAt,
            UpdatedAt = updatedAt
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        var artifact = Assert.Single(viewModel.Artifacts);
        Assert.Equal("/artifacts/brief.md", artifact.Path);
        Assert.Equal("1 个产物", viewModel.ArtifactSummary);
        viewModel.SelectedArtifact = artifact;
        Assert.Equal("full artifact content", viewModel.SelectedArtifactPreview);

        var session = Assert.Single(viewModel.MspSessions);
        Assert.Equal("artifact-session", session.Id);
        Assert.Equal("/artifacts/brief.md", Assert.Single(session.ArtifactPaths));

        var timelineArtifact = Assert.Single(viewModel.TimelineItems, item => item.Kind == TimelineItemKind.Artifact);
        Assert.Equal("/artifacts/brief.md", timelineArtifact.Artifact?.Path);
        Assert.Equal("artifact preview", timelineArtifact.Body);
    }

    [Fact]
    public async Task Selected_artifact_projects_lineage_and_opens_source_artifact()
    {
        var workspace = new WorkspaceState();
        var sourceArtifact = new WorkspaceArtifact
        {
            Id = "source-artifact",
            Path = "/artifacts/workflows/evidence.json",
            Content = "{ \"records\": [] }",
            Description = "Structured evidence",
            MediaType = "application/json",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 9, 0, 0, TimeSpan.Zero)
        };
        var derivedArtifact = new WorkspaceArtifact
        {
            Id = "derived-artifact",
            Path = "/artifacts/workflows/evidence-review.md",
            Content = "review content",
            Description = "Evidence review",
            MediaType = "text/markdown",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 0, TimeSpan.Zero)
        };
        derivedArtifact.SourcePaths.Add("/artifacts/workflows/evidence.json");
        derivedArtifact.SourcePaths.Add("/artifacts/workflows/evidence.json.manifest.json");
        derivedArtifact.SourcePaths.Add("/documents/doc-1/pages/2.txt");
        derivedArtifact.SourcePaths.Add("/documents/doc-1/pages/3.txt");
        derivedArtifact.SourceDocuments.Add("doc-1");
        derivedArtifact.SourcePages.Add("doc-1:2-3");
        workspace.Artifacts.Add(sourceArtifact);
        workspace.Artifacts.Add(derivedArtifact);

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = viewModel.Artifacts.Single(artifact => artifact.Id == "derived-artifact");

        Assert.True(viewModel.HasSelectedArtifactLineage);
        Assert.True(viewModel.Inspector.HasSelectedArtifactLineage);
        Assert.Same(viewModel.SelectedArtifactLineage, viewModel.Inspector.SelectedArtifactLineage);
        Assert.Contains(viewModel.SelectedArtifactLineage, item =>
            item.Path == "/artifacts/workflows/evidence.json" &&
            item.KindLabel == "Artifact" &&
            item.CanOpenArtifact);
        Assert.Contains(viewModel.SelectedArtifactLineage, item =>
            item.Path == "/artifacts/workflows/evidence.json.manifest.json" &&
            item.KindLabel == "Manifest");
        Assert.Contains(viewModel.SelectedArtifactLineage, item =>
            item.Path == "/documents/doc-1/pages/2.txt" &&
            item.KindLabel == "Page" &&
            item.Detail == "Document doc-1, page 2");
        Assert.Contains(viewModel.SelectedArtifactLineage, item =>
            item.Path == "doc-1" &&
            item.KindLabel == "Document");
        Assert.Contains(viewModel.SelectedArtifactLineage, item =>
            item.Path == "doc-1:2-3" &&
            item.KindLabel == "Pages" &&
            item.Detail == "Document doc-1, pages 2-3");

        var sourceLineageItem = viewModel.SelectedArtifactLineage.Single(item =>
            item.Path == "/artifacts/workflows/evidence.json");
        viewModel.OpenArtifactLineageItemCommand.Execute(sourceLineageItem);

        Assert.Equal("source-artifact", viewModel.SelectedArtifact?.Id);
        Assert.Equal(InspectorTab.Artifacts, viewModel.SelectedInspectorTab);
        Assert.False(viewModel.HasSelectedArtifactLineage);
        Assert.Empty(viewModel.SelectedArtifactLineage);
    }

    [Fact]
    public async Task Guided_outline_workflows_prepare_msp_command_draft()
    {
        var workspace = new WorkspaceState();
        var project = new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        };
        var document = new LibraryItem
        {
            Id = "doc-1",
            ProjectId = project.Id,
            Kind = LibraryItemKind.Pdf,
            Name = "Service Manual.pdf",
            RelativePath = "V:\\ReadOS-Test\\Library\\service-manual.pdf",
            PageCount = 12
        };
        document.Outline.Add(new OutlineItem
        {
            Id = "outline-32",
            Title = "3.2 Service Layer",
            Page = 3,
            Level = 2
        });
        project.LibraryItems.Add(document);
        workspace.Projects.Add(project);
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedOutlineItem = Assert.Single(viewModel.Outline);

        Assert.True(viewModel.HasSelectedOutlineWorkflowTarget);
        Assert.True(viewModel.Inspector.HasSelectedOutlineWorkflowTarget);

        viewModel.Inspector.PrepareExplainSectionWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run explain-section --document current --outline \"outline-32\" --artifact \"/artifacts/workflows/service-manual-3-2-service-layer-explanation.md\"",
            viewModel.MspCommandDraft);
        Assert.Equal(viewModel.MspCommandDraft, viewModel.Inspector.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);
        Assert.True(viewModel.IsInspectorVisible);

        viewModel.Inspector.PrepareExtractEvidenceWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run extract-evidence --document current --outline \"outline-32\" --artifact \"/artifacts/workflows/service-manual-3-2-service-layer-evidence.json\"",
            viewModel.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);
    }

    [Fact]
    public async Task Guided_artifact_workflows_prepare_msp_command_draft()
    {
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "evidence-artifact",
            Path = "/artifacts/workflows/evidence.json",
            Content = "{ \"workflow\": \"extract-evidence\" }",
            Description = "Structured evidence",
            MediaType = "application/json",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 9, 0, 0, TimeSpan.Zero)
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "synthesis-artifact",
            Path = "/artifacts/workflows/evidence-synthesis.md",
            Content = "synthesis",
            Description = "Evidence synthesis",
            MediaType = "text/markdown",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 0, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = viewModel.Artifacts.Single(artifact => artifact.Id == "evidence-artifact");

        Assert.True(viewModel.HasSelectedArtifactWorkflowTarget);
        Assert.True(viewModel.HasSelectedEvidenceArtifact);
        Assert.True(viewModel.Inspector.HasSelectedEvidenceArtifact);

        viewModel.Inspector.PrepareReviewEvidenceWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run review-evidence --evidence \"/artifacts/workflows/evidence.json\" --artifact \"/artifacts/workflows/evidence-review.md\"",
            viewModel.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);

        viewModel.Inspector.PrepareSynthesizeEvidenceWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run synthesize-evidence --evidence \"/artifacts/workflows/evidence.json\" --artifact \"/artifacts/workflows/evidence-synthesis.md\"",
            viewModel.MspCommandDraft);

        viewModel.SelectedArtifact = viewModel.Artifacts.Single(artifact => artifact.Id == "synthesis-artifact");

        Assert.True(viewModel.HasSelectedArtifactWorkflowTarget);
        Assert.False(viewModel.HasSelectedEvidenceArtifact);

        viewModel.Inspector.PrepareRefineArtifactWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run refine-artifact --source \"/artifacts/workflows/evidence-synthesis.md\" --instruction \"tighten caveats and keep citations\" --artifact \"/artifacts/workflows/evidence-synthesis-refined.md\"",
            viewModel.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);
    }

    [Fact]
    public async Task Composer_refinement_action_prepares_artifact_workflow_from_draft()
    {
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "synthesis-artifact",
            Path = "/artifacts/workflows/evidence-synthesis.md",
            Content = "synthesis",
            Description = "Evidence synthesis",
            MediaType = "text/markdown",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 0, TimeSpan.Zero)
        });
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        Assert.False(viewModel.HasComposerArtifactRefinementTarget);
        Assert.False(viewModel.Thread.HasComposerArtifactRefinementTarget);

        viewModel.SelectedArtifact = Assert.Single(viewModel.Artifacts);
        viewModel.ComposerDraft = "make the conclusion shorter and keep citations";

        Assert.True(viewModel.HasComposerArtifactRefinementTarget);
        Assert.True(viewModel.Thread.HasComposerArtifactRefinementTarget);

        var transcriptCount = viewModel.MspTranscript.Count;
        viewModel.Thread.PrepareComposerArtifactRefinementWorkflowCommand.Execute(null);

        Assert.Equal(
            "workflow run refine-artifact --source \"/artifacts/workflows/evidence-synthesis.md\" --instruction \"make the conclusion shorter and keep citations\" --artifact \"/artifacts/workflows/evidence-synthesis-steered.md\"",
            viewModel.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);
        Assert.True(viewModel.IsInspectorVisible);
        Assert.Equal(transcriptCount, viewModel.MspTranscript.Count);
        Assert.Equal("make the conclusion shorter and keep citations", viewModel.ComposerDraft);
        Assert.Contains("composer 精炼工作流", viewModel.StatusMessage);
        Assert.Contains(viewModel.PreparedMspCommands, command => command.CommandText == viewModel.MspCommandDraft);
    }

    [Fact]
    public async Task Prepared_workflow_commands_are_recorded_and_restored_without_running()
    {
        var workspace = new WorkspaceState();
        var project = new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        };
        var document = new LibraryItem
        {
            Id = "doc-1",
            ProjectId = project.Id,
            Kind = LibraryItemKind.Pdf,
            Name = "Service Manual.pdf",
            RelativePath = "V:\\ReadOS-Test\\Library\\service-manual.pdf",
            PageCount = 12
        };
        document.Outline.Add(new OutlineItem
        {
            Id = "outline-32",
            Title = "3.2 Service Layer",
            Page = 3,
            Level = 2
        });
        project.LibraryItems.Add(document);
        workspace.Projects.Add(project);
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedOutlineItem = Assert.Single(viewModel.Outline);

        viewModel.Inspector.PrepareExplainSectionWorkflowCommand.Execute(null);
        var explainCommand = viewModel.MspCommandDraft;
        viewModel.Inspector.PrepareExtractEvidenceWorkflowCommand.Execute(null);

        Assert.True(viewModel.HasPreparedMspCommands);
        Assert.True(viewModel.Inspector.HasPreparedMspCommands);
        Assert.Same(viewModel.PreparedMspCommands, viewModel.Inspector.PreparedMspCommands);
        Assert.Equal(2, viewModel.PreparedMspCommands.Count);
        Assert.Contains("extract-evidence", viewModel.PreparedMspCommands[0].CommandText);
        Assert.Contains("explain-section", viewModel.PreparedMspCommands[1].CommandText);

        var transcriptCount = viewModel.MspTranscript.Count;
        viewModel.MspCommandDraft = "workspace info";

        viewModel.Inspector.RestorePreparedMspCommandCommand.Execute(viewModel.PreparedMspCommands[1]);

        Assert.Equal(explainCommand, viewModel.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);
        Assert.True(viewModel.IsInspectorVisible);
        Assert.Equal(transcriptCount, viewModel.MspTranscript.Count);
        Assert.Contains("explain-section", viewModel.PreparedMspCommands[0].CommandText);
        Assert.Contains("已恢复命令草稿", viewModel.StatusMessage);
    }

    [Fact]
    public async Task Prepared_workflow_command_history_is_capped_and_deduplicated()
    {
        var workspace = new WorkspaceState();
        var project = new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        };
        var document = new LibraryItem
        {
            Id = "doc-1",
            ProjectId = project.Id,
            Kind = LibraryItemKind.Pdf,
            Name = "Service Manual.pdf",
            RelativePath = "V:\\ReadOS-Test\\Library\\service-manual.pdf",
            PageCount = 24
        };
        for (var index = 1; index <= 9; index++)
        {
            document.Outline.Add(new OutlineItem
            {
                Id = $"outline-{index}",
                Title = $"Section {index}",
                Page = index,
                Level = 1
            });
        }

        project.LibraryItems.Add(document);
        workspace.Projects.Add(project);
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        foreach (var outlineItem in viewModel.Outline)
        {
            viewModel.SelectedOutlineItem = outlineItem;
            viewModel.Inspector.PrepareExplainSectionWorkflowCommand.Execute(null);
        }

        Assert.Equal(8, viewModel.PreparedMspCommands.Count);
        Assert.Contains("outline-9", viewModel.PreparedMspCommands[0].CommandText);
        Assert.DoesNotContain(viewModel.PreparedMspCommands, command => command.CommandText.Contains("outline-1"));

        viewModel.SelectedOutlineItem = viewModel.Outline.Single(item => item.Id == "outline-5");
        viewModel.Inspector.PrepareExplainSectionWorkflowCommand.Execute(null);

        Assert.Equal(8, viewModel.PreparedMspCommands.Count);
        Assert.Contains("outline-5", viewModel.PreparedMspCommands[0].CommandText);
        Assert.Equal(1, viewModel.PreparedMspCommands.Count(command => command.CommandText.Contains("outline-5")));
    }

    [Fact]
    public async Task Artifact_actions_open_copy_and_export_selected_artifact()
    {
        var exportPath = Path.Combine(Path.GetTempPath(), $"reados-artifact-{Guid.NewGuid():N}.md");
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/workflows/evidence-review.md",
            Content = "# Review\n\ncontent",
            Description = "Evidence review",
            MediaType = "text/markdown",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 9, 0, 0, TimeSpan.Zero)
        });
        var fileDialog = new TestFileDialogService
        {
            ArtifactExportPath = exportPath
        };
        var clipboard = new TestClipboardService();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            fileDialog,
            new TestAiChatService(),
            clipboard);

        try
        {
            await viewModel.InitializeAsync();
            viewModel.SelectedArtifact = Assert.Single(viewModel.Artifacts);

            viewModel.Inspector.OpenSelectedArtifactPreviewCommand.Execute(null);

            Assert.Equal(InspectorTab.Preview, viewModel.SelectedInspectorTab);
            Assert.True(viewModel.HasTextPresenter);
            Assert.False(viewModel.HasPageImage);
            Assert.Equal("Markdown 预览", viewModel.PresenterKindLabel);
            Assert.Equal("# Review\n\ncontent", viewModel.PresenterTextContent);

            await viewModel.Inspector.CopySelectedArtifactContentCommand.ExecuteAsync(null);

            Assert.Equal("# Review\n\ncontent", clipboard.Text);
            Assert.Contains("已复制产物内容", viewModel.StatusMessage);

            await viewModel.Inspector.ExportSelectedArtifactCommand.ExecuteAsync(null);

            Assert.Equal("evidence-review.md", fileDialog.LastArtifactSuggestedName);
            Assert.Equal(".md", fileDialog.LastArtifactExtension);
            Assert.True(File.Exists(exportPath));
            Assert.Equal("# Review\n\ncontent", await File.ReadAllTextAsync(exportPath));
            Assert.Contains("产物已导出", viewModel.StatusMessage);
        }
        finally
        {
            if (File.Exists(exportPath))
            {
                File.Delete(exportPath);
            }
        }
    }

    [Fact]
    public async Task Search_filters_sessions_and_artifacts()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "transcript-beta",
            SessionId = "beta-session",
            Actor = "agent",
            CommandText = "workspace status",
            StartedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 1, TimeSpan.Zero),
            Decision = "Allow"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-alpha",
            Path = "/artifacts/alpha-report.md",
            Content = "alpha artifact content",
            Description = "Alpha report",
            SourceCommand = "artifact write /artifacts/alpha-report.md",
            SessionId = "alpha-session",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 11, 0, 0, TimeSpan.Zero)
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        Assert.Equal(2, viewModel.MspSessions.Count);
        Assert.Single(viewModel.Artifacts);

        viewModel.SearchQuery = "alpha";

        var alphaSession = Assert.Single(viewModel.MspSessions);
        Assert.Equal("alpha-session", alphaSession.Id);
        Assert.Equal("/artifacts/alpha-report.md", Assert.Single(viewModel.Artifacts).Path);

        viewModel.SearchQuery = "workspace";

        var betaSession = Assert.Single(viewModel.MspSessions);
        Assert.Equal("beta-session", betaSession.Id);
        Assert.Empty(viewModel.Artifacts);
    }

    [Fact]
    public async Task Attach_selected_artifact_queues_virtual_file_attachment()
    {
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/brief.md",
            Content = "full artifact content",
            Description = "Brief",
            SessionId = "artifact-session"
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = Assert.Single(viewModel.Artifacts);

        viewModel.AttachSelectedArtifactCommand.Execute(null);

        var attachment = Assert.Single(viewModel.PendingAttachments);
        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal("/artifacts/brief.md", attachment.FilePath);
        Assert.Equal("产物 · /artifacts/brief.md", attachment.Title);
        Assert.Equal("请基于我附加的产物继续分析。", viewModel.ComposerDraft);
        Assert.Equal(InspectorTab.Evidence, viewModel.SelectedInspectorTab);
    }

    [Fact]
    public async Task Remove_pending_attachment_updates_queue_and_send_snapshot()
    {
        var workspace = new WorkspaceState();
        workspace.Projects.Add(new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/first.md",
            Content = "first content",
            Description = "First",
            SessionId = "artifact-session"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-2",
            Path = "/artifacts/second.md",
            Content = "second content",
            Description = "Second",
            SessionId = "artifact-session"
        });
        var chatService = new TestAiChatService();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            chatService);

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = viewModel.Artifacts.Single(artifact => artifact.Path == "/artifacts/first.md");
        viewModel.AttachSelectedArtifactCommand.Execute(null);
        var removed = Assert.Single(viewModel.PendingAttachments);
        viewModel.SelectedArtifact = viewModel.Artifacts.Single(artifact => artifact.Path == "/artifacts/second.md");
        viewModel.AttachSelectedArtifactCommand.Execute(null);

        viewModel.RemovePendingAttachmentCommand.Execute(removed);

        var remaining = Assert.Single(viewModel.PendingAttachments);
        Assert.Equal("/artifacts/second.md", remaining.FilePath);
        Assert.Equal("1 个附件待发送", viewModel.PendingAttachmentSummary);
        Assert.True(viewModel.Thread.HasPendingAttachments);

        await viewModel.SendPromptCommand.ExecuteAsync(null);

        Assert.Equal("/artifacts/second.md", Assert.Single(chatService.LastAttachments).FilePath);
        Assert.Equal("second content", Assert.Single(chatService.LastAttachmentTexts));
        Assert.Empty(viewModel.PendingAttachments);
        Assert.False(viewModel.Thread.HasPendingAttachments);
    }

    [Fact]
    public async Task Composer_primary_action_routes_to_send_or_stop_by_runtime_state()
    {
        var workspace = new WorkspaceState();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        Assert.Same(viewModel.SendPromptCommand, viewModel.ComposerPrimaryCommand);
        Assert.Same(viewModel.SendPromptCommand, viewModel.Thread.ComposerPrimaryCommand);
        Assert.Equal("发送", viewModel.SendButtonLabel);
        Assert.Equal("\uE724", viewModel.ComposerPrimaryGlyph);

        viewModel.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "running-command",
            CommandText = "pdf search current alpha",
            IsRunning = true
        });

        Assert.Same(viewModel.CancelActiveMspCommandCommand, viewModel.ComposerPrimaryCommand);
        Assert.Same(viewModel.CancelActiveMspCommandCommand, viewModel.Thread.ComposerPrimaryCommand);
        Assert.Equal("停止", viewModel.Thread.SendButtonLabel);
        Assert.Equal("\uE711", viewModel.Thread.ComposerPrimaryGlyph);
        Assert.Equal("停止当前 MSP 命令", viewModel.Thread.ComposerPrimaryToolTip);
    }

    [Fact]
    public async Task Queue_composer_draft_preserves_prompt_and_attachments_for_restore()
    {
        var workspace = new WorkspaceState();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "running-command",
            CommandText = "pdf search current alpha",
            IsRunning = true
        });
        viewModel.ComposerDraft = "Please analyze this later.";
        viewModel.PendingAttachments.Add(new ChatAttachment
        {
            Id = "attachment-1",
            Kind = AttachmentKind.File,
            Title = "Artifact",
            FilePath = "/artifacts/brief.md"
        });

        viewModel.Thread.QueueComposerDraftCommand.Execute(null);

        var queued = Assert.Single(viewModel.QueuedComposerPrompts);
        Assert.Equal("Please analyze this later.", queued.Prompt);
        Assert.Equal("/artifacts/brief.md", Assert.Single(queued.Attachments).FilePath);
        Assert.Empty(viewModel.ComposerDraft);
        Assert.Empty(viewModel.PendingAttachments);
        Assert.True(viewModel.Thread.HasQueuedComposerPrompts);
        Assert.Equal("1 个队列项", viewModel.Thread.ComposerQueueSummary);

        viewModel.Thread.RestoreQueuedComposerPromptCommand.Execute(queued);

        Assert.Empty(viewModel.QueuedComposerPrompts);
        Assert.Equal("Please analyze this later.", viewModel.ComposerDraft);
        Assert.Equal("/artifacts/brief.md", Assert.Single(viewModel.PendingAttachments).FilePath);
        Assert.False(viewModel.Thread.HasQueuedComposerPrompts);
        Assert.Equal("暂无队列", viewModel.Thread.ComposerQueueSummary);
    }

    [Fact]
    public async Task Artifact_attachment_content_is_sent_to_chat_service()
    {
        var workspace = new WorkspaceState();
        workspace.Projects.Add(new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/brief.md",
            Content = "full artifact content",
            Description = "Brief",
            SessionId = "artifact-session"
        });
        var chatService = new TestAiChatService();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            chatService);

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = Assert.Single(viewModel.Artifacts);
        viewModel.AttachSelectedArtifactCommand.Execute(null);

        await viewModel.SendPromptCommand.ExecuteAsync(null);

        Assert.Equal("请基于我附加的产物继续分析。", chatService.LastPrompt);
        Assert.Equal("/artifacts/brief.md", Assert.Single(chatService.LastAttachments).FilePath);
        Assert.Equal("full artifact content", Assert.Single(chatService.LastAttachmentTexts));
        var conversation = Assert.Single(workspace.Projects[0].StandaloneConversations);
        Assert.Equal(2, conversation.Messages.Count);
    }

    [Fact]
    public async Task Chat_provider_failure_withholds_provider_response_details()
    {
        var workspace = new WorkspaceState();
        workspace.Projects.Add(new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        });
        const string sensitiveFragment = "provider echoed sk-live-secret-12345";
        var chatService = new TestAiChatService
        {
            Response = "unused",
            Failure = new AiChatServiceException($"provider returned 500: {sensitiveFragment}")
        };
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            chatService);

        await viewModel.InitializeAsync();
        viewModel.ComposerDraft = "explain the document";

        await viewModel.SendPromptCommand.ExecuteAsync(null);

        Assert.DoesNotContain(sensitiveFragment, viewModel.StatusMessage, StringComparison.Ordinal);
        Assert.Contains("已隐藏提供方响应详情", viewModel.StatusMessage, StringComparison.Ordinal);
        var conversation = Assert.Single(workspace.Projects[0].StandaloneConversations);
        var failureMessage = Assert.Single(conversation.Messages, message => message.Role == ChatRole.Assistant);
        Assert.DoesNotContain(sensitiveFragment, failureMessage.Content, StringComparison.Ordinal);
        Assert.Contains("已隐藏提供方响应详情", failureMessage.Content, StringComparison.Ordinal);
    }

    [Fact]
    public async Task File_attachment_with_crafted_host_path_does_not_read_arbitrary_file()
    {
        var workspace = new WorkspaceState();
        workspace.Projects.Add(new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        });
        var chatService = new TestAiChatService();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            chatService);

        await viewModel.InitializeAsync();
        viewModel.PendingAttachments.Add(new ChatAttachment
        {
            Id = "crafted-attachment",
            Kind = AttachmentKind.File,
            Title = "Crafted",
            FilePath = "C:\\secret.txt",
            DocumentId = string.Empty
        });

        await viewModel.SendPromptCommand.ExecuteAsync(null);

        Assert.Equal(string.Empty, Assert.Single(chatService.LastAttachmentTexts));
        var conversation = Assert.Single(workspace.Projects[0].StandaloneConversations);
        Assert.Equal(2, conversation.Messages.Count);
    }

    private sealed class TestWorkspaceStore : IWorkspaceStore
    {
        private readonly WorkspaceState workspace;

        public TestWorkspaceStore(WorkspaceState workspace)
        {
            this.workspace = workspace;
        }

        public int SaveCount { get; private set; }

        public string WorkspaceRoot => "V:\\ReadOS-Test";

        public string LibraryRoot => "V:\\ReadOS-Test\\Library";

        public Task<WorkspaceState> LoadAsync(CancellationToken cancellationToken = default)
        {
            return Task.FromResult(workspace);
        }

        public Task SaveAsync(WorkspaceState state, CancellationToken cancellationToken = default)
        {
            SaveCount++;
            return Task.CompletedTask;
        }

        public Task<ProjectItem> CreateProjectAsync(WorkspaceState state, string name, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<LibraryItem> ImportDocumentAsync(WorkspaceState state, ProjectItem project, string sourcePath, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task RenameDocumentAsync(WorkspaceState state, LibraryItem document, string newName, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task DeleteDocumentAsync(WorkspaceState state, ProjectItem project, LibraryItem document, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<string> ExportWorkspaceAsync(WorkspaceState state, string destinationPath, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<WorkspaceState> ImportWorkspaceAsync(string sourcePath, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public string GetAbsolutePath(LibraryItem item)
        {
            return item.RelativePath;
        }
    }

    private sealed class TestPdfDocumentService : IPdfDocumentService
    {
        public Task<PdfDocumentInfo> InspectAsync(string path, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<BitmapImage> RenderPageAsync(string path, int pageNumber, double width, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(string path, int pageCount, int maxPages, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<IReadOnlyList<PdfTextHit>> SearchAsync(string path, string query, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<string> ExtractPageTextAsync(string path, int startPage, int endPage, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }
    }

    private sealed class TestFileDialogService : IFileDialogService
    {
        public string? ArtifactExportPath { get; set; }

        public string LastArtifactSuggestedName { get; private set; } = string.Empty;

        public string LastArtifactExtension { get; private set; } = string.Empty;

        public Task<IReadOnlyList<string>> PickPdfFilesAsync(Window window)
        {
            return Task.FromResult<IReadOnlyList<string>>(Array.Empty<string>());
        }

        public Task<string?> PickWorkspaceImportAsync(Window window)
        {
            return Task.FromResult<string?>(null);
        }

        public Task<string?> PickWorkspaceExportAsync(Window window, string suggestedName)
        {
            return Task.FromResult<string?>(null);
        }

        public Task<string?> PickArtifactExportAsync(Window? window, string suggestedName, string extension)
        {
            LastArtifactSuggestedName = suggestedName;
            LastArtifactExtension = extension;
            return Task.FromResult(ArtifactExportPath);
        }
    }

    private sealed class TestClipboardService : IClipboardService
    {
        public string Text { get; private set; } = string.Empty;

        public Task SetTextAsync(string text, CancellationToken cancellationToken = default)
        {
            Text = text;
            return Task.CompletedTask;
        }
    }

    private sealed class TestAiChatService : IAiChatService
    {
        public string Response { get; set; } = "unused";

        public Exception? Failure { get; set; }

        public string LastPrompt { get; private set; } = string.Empty;

        public IReadOnlyList<ChatAttachment> LastAttachments { get; private set; } = Array.Empty<ChatAttachment>();

        public IReadOnlyList<string> LastAttachmentTexts { get; private set; } = Array.Empty<string>();

        public async Task<string> SendAsync(
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
            if (Failure is not null)
            {
                throw Failure;
            }

            LastPrompt = userPrompt;
            var capturedAttachments = attachments.ToArray();
            LastAttachments = capturedAttachments;

            var attachmentTexts = new List<string>();
            foreach (var attachment in capturedAttachments)
            {
                attachmentTexts.Add(await attachmentTextProvider(attachment));
            }

            LastAttachmentTexts = attachmentTexts;
            return Response;
        }
    }

    [Fact]
    public async Task Flagship_evidence_and_synthesis_workflows_are_visible_through_shell()
    {
        var workspace = new WorkspaceState();
        var project = new ProjectItem { Id = "project-1", Name = "Project" };
        var document = new LibraryItem
        {
            Id = "doc-1",
            ProjectId = project.Id,
            Kind = LibraryItemKind.Pdf,
            Name = "Service Manual.pdf",
            RelativePath = "V:\\ReadOS-Test\\Library\\service-manual.pdf",
            PageCount = 8
        };
        document.Outline.Add(new OutlineItem
        {
            Id = "section-3-2",
            Title = "3.2 Service Layer",
            Page = 4,
            Level = 2
        });
        project.LibraryItems.Add(document);
        workspace.Projects.Add(project);

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new FlagshipPdfDocumentService(),
            new TestFileDialogService(),
            new FlagshipAiChatService());

        await viewModel.InitializeAsync();

        // Operator selects the outline section in the inspector and prepares the evidence command.
        viewModel.SelectedOutlineItem = viewModel.Outline.Single(item => item.Title == "3.2 Service Layer");
        viewModel.Inspector.PrepareExtractEvidenceWorkflowCommand.Execute(null);

        Assert.Contains("extract-evidence", viewModel.MspCommandDraft);
        Assert.Equal(InspectorTab.Run, viewModel.SelectedInspectorTab);

        // Run -> pending approval (surfaced in the approvals flyout / top-bar badge).
        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var pendingExtract = viewModel.MspTranscript.Single(entry => entry.CommandText.Contains("extract-evidence"));
        Assert.True(viewModel.HasPendingApprovals);
        Assert.Equal("RequireConfirmation", pendingExtract.Decision);

        // Operator approves the artifact write from the shell.
        await viewModel.ApproveMspCommandCommand.ExecuteAsync(pendingExtract);

        var extractEntry = viewModel.MspTranscript.Single(entry => entry.CommandText.Contains("extract-evidence"));
        Assert.True(extractEntry.Succeeded, extractEntry.Stderr);
        Assert.False(viewModel.HasPendingApprovals);

        var evidenceArtifact = Assert.Single(
            workspace.Artifacts,
            artifact => artifact.Path.Contains("evidence.json"));
        Assert.Contains("page four evidence", evidenceArtifact.Content);

        // Operator selects the evidence artifact and prepares the synthesis command.
        viewModel.SelectedArtifact = evidenceArtifact;
        viewModel.Inspector.PrepareSynthesizeEvidenceWorkflowCommand.Execute(null);

        Assert.Contains("synthesize-evidence", viewModel.MspCommandDraft);

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var pendingSynthesis = viewModel.MspTranscript.Single(entry => entry.CommandText.Contains("synthesize-evidence"));
        Assert.True(viewModel.HasPendingApprovals);

        await viewModel.ApproveMspCommandCommand.ExecuteAsync(pendingSynthesis);

        var synthesisEntry = viewModel.MspTranscript.Single(entry => entry.CommandText.Contains("synthesize-evidence"));
        Assert.True(synthesisEntry.Succeeded, synthesisEntry.Stderr);

        var synthesisArtifact = Assert.Single(
            workspace.Artifacts,
            artifact => artifact.Path.Contains("evidence-synthesis.md"));
        Assert.Contains(evidenceArtifact.Path, synthesisArtifact.SourcePaths);
    }

    private sealed class FlagshipPdfDocumentService : IPdfDocumentService
    {
        private static readonly IReadOnlyDictionary<int, string> PageText = new Dictionary<int, string>
        {
            [4] = "page four evidence",
            [5] = "page five evidence",
            [6] = "page six evidence"
        };

        public Task<PdfDocumentInfo> InspectAsync(string path, CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(new PdfDocumentInfo(
                8,
                new[] { new PageLabelRule { PdfPage = 4, Label = "S-1" } },
                new[]
                {
                    new OutlineItem { Id = "chapter-3", Title = "3 Architecture", Page = 2, Level = 1 },
                    new OutlineItem { Id = "section-3-2", Title = "3.2 Service Layer", Page = 4, Level = 2 }
                }));
        }

        public Task<BitmapImage> RenderPageAsync(string path, int pageNumber, double width, CancellationToken cancellationToken = default)
            => throw new NotSupportedException();

        public Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(string path, int pageCount, int maxPages, CancellationToken cancellationToken = default)
            => Task.FromResult<IReadOnlyList<PageImageItem>>(Array.Empty<PageImageItem>());

        public Task<IReadOnlyList<PdfTextHit>> SearchAsync(string path, string query, CancellationToken cancellationToken = default)
            => Task.FromResult<IReadOnlyList<PdfTextHit>>(Array.Empty<PdfTextHit>());

        public Task<string> ExtractPageTextAsync(string path, int startPage, int endPage, CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(PageText.TryGetValue(startPage, out var text) ? text : string.Empty);
        }
    }

    private sealed class FlagshipAiChatService : IAiChatService
    {
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
            return Task.FromResult("Synthesized durable MSP boundary evidence.");
        }
    }
}

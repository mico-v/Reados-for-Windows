using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspHostTests
{
    [Fact]
    public void Command_pack_factory_returns_reados_app_command_names()
    {
        var workspace = CreateWorkspace(out var document);
        var factory = CreateCommandPackFactory(workspace, document);

        var commandPack = factory.CreateCommandPack();

        Assert.Equal("ReadOS app commands", commandPack.Name);
        Assert.Equal(new[]
        {
            "workspace",
            "library",
            "pdf",
            "windows",
            "page-label",
            "outline",
            "attach",
            "chat",
            "workflow"
        }, commandPack.CommandNames);
        Assert.Equal(commandPack.CommandNames.Count, commandPack.Commands.Count);
    }

    [Fact]
    public void Command_pack_workflow_command_overrides_core_workflow_in_host_composition()
    {
        var workspace = CreateWorkspace(out var document);
        var commandPack = CreateCommandPackFactory(workspace, document).CreateCommandPack();
        var appWorkflow = commandPack.Commands.Single(command => command.Name == "workflow");

        var composition = new MspCommandHostCompositionBuilder().Build(commandPack);

        Assert.Contains("workflow", composition.CoreCommandNames);
        Assert.Equal("ReadOS app commands", composition.HostCommandPackName);
        Assert.Contains("workflow", composition.HostCommandNames);
        Assert.Equal(new[] { "workflow" }, composition.OverriddenCoreCommandNames);
        Assert.True(composition.Registry.TryGet("workflow", out var command));
        Assert.Same(appWorkflow, command);
        Assert.Equal(1, composition.CommandNames.Count(name =>
            string.Equals(name, "workflow", StringComparison.OrdinalIgnoreCase)));
    }

    [Fact]
    public async Task Host_runtime_factory_composes_request_defaults_commands_and_runtime()
    {
        var workspace = CreateWorkspace(out var document);
        var dependencies = CreateHostDependencies(workspace, document);
        var factory = new ReadOsMspHostRuntimeFactory();

        var hostRuntime = factory.Create(
            dependencies,
            ReadOsMspHost.DefaultSessionId,
            "reados-agent");

        Assert.Equal(ReadOsMspHost.DefaultSessionId, hostRuntime.RequestFactory.DefaultSessionId);
        Assert.Equal("reados-agent", hostRuntime.RequestFactory.DefaultActor);
        Assert.Equal("ReadOS app commands", hostRuntime.Composition.HostCommandPackName);
        Assert.Contains("workspace", hostRuntime.Composition.HostCommandNames);
        Assert.Equal(new[] { "workflow" }, hostRuntime.Composition.OverriddenCoreCommandNames);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, hostRuntime.Diagnostics.DefaultSessionId);
        Assert.Equal("reados-agent", hostRuntime.Diagnostics.DefaultActor);
        Assert.Equal("ReadOS app commands", hostRuntime.Diagnostics.HostCommandPackName);
        Assert.Contains("workspace", hostRuntime.Diagnostics.HostCommandNames);
        Assert.Equal(new[] { "workflow" }, hostRuntime.Diagnostics.OverriddenCoreCommandNames);
        Assert.True(hostRuntime.Diagnostics.HasCoreOverrides);

        var result = await hostRuntime.CommandHost.ExecuteAsync(
            hostRuntime.RequestFactory.Create("workspace info"));

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Contains("workspaceRoot\t<workspace>", result.Stdout);
        var audit = Assert.Single(result.AuditRecords);
        Assert.Equal("reados-agent", audit.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, audit.SessionId);
        Assert.Equal("workspace", audit.CommandName);
    }

    [Fact]
    public async Task Host_runtime_factory_protects_pwd_echo_ls_and_cat_as_lazy_native_owned_commands()
    {
        var workspace = CreateWorkspace(out var document);
        var dependencies = CreateHostDependencies(workspace, document);
        var provider = new TrackingNativeAdapterProvider();
        using var hostRuntime = new ReadOsMspHostRuntimeFactory().Create(
            dependencies,
            ReadOsMspHost.DefaultSessionId,
            "reados-agent",
            provider,
            ownsNativeAdapterProvider: true);

        foreach (var commandName in new[] { "pwd", "PWD", "echo", "EcHo", "ls", "LS", "cat", "CAT" })
        {
            Assert.True(hostRuntime.Composition.Registry.TryGet(commandName, out var command));
            Assert.IsType<MspNativeBackedCommand>(command);
        }

        foreach (var commandName in new[] { "help", "artifact", "workflow", "workspace" })
        {
            Assert.True(hostRuntime.Composition.Registry.TryGet(commandName, out var command));
            Assert.IsNotType<MspNativeBackedCommand>(command);
        }

        var result = await hostRuntime.CommandHost.ExecuteAsync(
            hostRuntime.RequestFactory.Create("workspace info"));

        Assert.True(result.Succeeded, result.Stderr);
        Assert.False(provider.IsAdapterCreated);
        Assert.Equal(0, provider.DisposeCalls);

        hostRuntime.Dispose();
        hostRuntime.Dispose();
        Assert.Equal(1, provider.DisposeCalls);
    }

    [Fact]
    public async Task Host_runtime_factory_registers_explicit_ffi_echo_only_and_keeps_legacy_routes()
    {
        var workspace = CreateWorkspace(out var document);
        var echoAdapter = new MspCommandRuntimeFfiEchoCommandAdapter(null);
        var dependencies = CreateHostDependencies(
            workspace,
            document,
            runtimeFfiEchoCommandAdapter: echoAdapter);
        var provider = new TrackingNativeAdapterProvider();
        using var hostRuntime = new ReadOsMspHostRuntimeFactory().Create(
            dependencies,
            ReadOsMspHost.DefaultSessionId,
            "reados-agent",
            provider,
            ownsNativeAdapterProvider: true);

        Assert.True(hostRuntime.Composition.Registry.TryGet("echo", out var echo));
        Assert.Same(echoAdapter, echo);
        foreach (var commandName in new[] { "pwd", "ls", "cat" })
        {
            Assert.True(hostRuntime.Composition.Registry.TryGet(commandName, out var command));
            Assert.IsType<MspNativeBackedCommand>(command);
        }

        var unavailable = await hostRuntime.CommandHost.ExecuteAsync(
            hostRuntime.RequestFactory.Create("echo explicitly opted in"));

        Assert.Equal(
            MspCommandRuntimeFfiEchoCommandAdapter.AdapterUnavailableDiagnosticCode,
            Assert.Single(unavailable.Diagnostics).Code);
        Assert.Single(unavailable.AuditRecords);
        Assert.Equal("echo", unavailable.AuditRecords[0].CommandName);
        Assert.False(provider.IsAdapterCreated);

        hostRuntime.Dispose();
        hostRuntime.Dispose();
        Assert.Equal(1, provider.DisposeCalls);
    }

    [Theory]
    [InlineData("pwd")]
    [InlineData("PWD")]
    [InlineData("echo")]
    [InlineData("EcHo")]
    [InlineData("ls")]
    [InlineData("LS")]
    [InlineData("cat")]
    [InlineData("CAT")]
    public void Native_core_registry_rejects_host_overrides_of_protected_commands(
        string commandName)
    {
        var commandPack = new MspCommandPack(
            "Hostile overrides",
            new[] { new ProtectedNameTestCommand(commandName) });

        var exception = Assert.Throws<InvalidOperationException>(() =>
            ReadOsNativeCoreRegistryFactory.ValidateHostCommandPack(commandPack));

        Assert.Contains("cannot override native-owned command", exception.Message);
    }

    [Fact]
    public async Task Host_created_from_dependencies_executes_app_command_pack()
    {
        var workspace = CreateWorkspace(out var document);
        var dependencies = CreateHostDependencies(workspace, document);
        var host = new ReadOsMspHost(dependencies);

        var result = await host.ExecuteAsync("workspace info", "test-agent");

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Contains("workspaceRoot\t<workspace>", result.Stdout);
        Assert.Contains("documents\t1", result.Stdout);
    }

    [Fact]
    public void Host_dependencies_reject_missing_app_services()
    {
        var workspace = CreateWorkspace(out var document);

        Assert.Throws<ArgumentNullException>(() => new ReadOsMspHostDependencies(
            null!,
            new TestPdfDocumentService(),
            new TestAiChatService("unused"),
            () => workspace,
            () => workspace.Settings,
            () => document,
            () => Array.Empty<ChatAttachment>(),
            _ => Task.FromResult("attachment text"),
            _ => { },
            () => { },
            (_, _) => { }));
    }

    [Fact]
    public async Task Attach_page_requires_approval_before_queueing_attachment()
    {
        var workspace = CreateWorkspace(out var document);
        var attachments = new List<ChatAttachment>();
        var host = CreateHost(workspace, document, attachmentSink: attachments.Add);

        var pending = await host.ExecuteAsync("attach page current 3", "test-agent");

        Assert.Empty(attachments);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Contains("Queue a PDF page", pendingAudit.Preview.Summary);
        Assert.Contains(document.Id, Assert.Single(pendingAudit.Preview.Targets));
        Assert.Contains("page: 3", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync("attach page current 3", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);

        var attachment = Assert.Single(attachments);
        Assert.Equal(AttachmentKind.Page, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal(3, attachment.StartPage);
        Assert.Equal(3, attachment.EndPage);
    }

    [Fact]
    public async Task Allow_workspace_approval_mode_allows_workspace_writes_without_pending_approval()
    {
        var workspace = CreateWorkspace(out var document);
        workspace.Settings.MspApprovalMode = MspApprovalModeCodes.AllowWorkspace;
        var attachments = new List<ChatAttachment>();
        var host = CreateHost(workspace, document, attachmentSink: attachments.Add);

        var result = await host.ExecuteAsync("attach page current 3", "test-agent");

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(result.AuditRecords).Decision);
        Assert.Equal(AttachmentKind.Page, Assert.Single(attachments).Kind);
    }

    [Fact]
    public async Task Artifact_delete_requires_approval_and_removes_workspace_artifact_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            Content = "# Report",
            MediaType = "text/markdown"
        });
        var host = CreateHost(workspace, document);

        var pending = await host.ExecuteAsync("artifact delete /artifacts/report.md", "test-agent");

        Assert.False(pending.Succeeded);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, Assert.Single(pending.AuditRecords).Decision);
        Assert.Equal(MspCommandEffects.DeleteWorkspace, pending.AuditRecords[0].Effects);
        Assert.Single(workspace.Artifacts);

        var approved = await host.ExecuteApprovedAsync("artifact delete /artifacts/report.md", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);
        Assert.Empty(workspace.Artifacts);
    }

    [Fact]
    public async Task Artifact_rename_requires_approval_and_preserves_content_and_manifest_fields()
    {
        var workspace = CreateWorkspace(out var document);
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            Content = "# Report",
            MediaType = "text/markdown",
            Description = "Report",
            SourceCommand = "artifact write /artifacts/report.md report",
            Actor = "source-agent",
            SessionId = "source-session",
            Preview = "contentLength: 8"
        });
        var host = CreateHost(workspace, document);

        var pending = await host.ExecuteAsync("artifact rename /artifacts/report.md /artifacts/archive/report.md", "test-agent");

        Assert.False(pending.Succeeded);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, Assert.Single(pending.AuditRecords).Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.DeleteWorkspace,
            pending.AuditRecords[0].Effects);
        Assert.Equal("/artifacts/report.md", Assert.Single(workspace.Artifacts).Path);

        var approved = await host.ExecuteApprovedAsync("artifact rename /artifacts/report.md /artifacts/archive/report.md", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        var artifact = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/archive/report.md", artifact.Path);
        Assert.Equal("# Report", artifact.Content);
        Assert.Equal("text/markdown", artifact.MediaType);
        Assert.Equal("test-agent", artifact.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, artifact.SessionId);
        Assert.Equal("artifact rename /artifacts/report.md /artifacts/archive/report.md", artifact.SourceCommand);
        Assert.Contains("renamedFrom: /artifacts/report.md", artifact.Preview);
    }

    [Fact]
    public async Task Confirm_all_approval_mode_requires_confirmation_for_read_commands()
    {
        var workspace = CreateWorkspace(out var document);
        workspace.Settings.MspApprovalMode = MspApprovalModeCodes.ConfirmAll;
        var host = CreateHost(workspace, document);

        var pending = await host.ExecuteAsync("workspace info", "test-agent");

        Assert.False(pending.Succeeded);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, Assert.Single(pending.AuditRecords).Decision);

        var approved = await host.ExecuteApprovedAsync("workspace info", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);
    }

    [Fact]
    public async Task Attach_range_queues_page_range_attachment_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        var attachments = new List<ChatAttachment>();
        var host = CreateHost(workspace, document, attachmentSink: attachments.Add);

        var result = await host.ExecuteApprovedAsync($"attach range {document.Id} 7 4", "test-agent");

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Contains($"attached-range\t{document.Id}\t4\t7", result.Stdout);

        var attachment = Assert.Single(attachments);
        Assert.Equal(AttachmentKind.PageRange, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal(4, attachment.StartPage);
        Assert.Equal(7, attachment.EndPage);
    }

    [Fact]
    public async Task Chat_ask_requires_approval_before_writing_conversation()
    {
        var workspace = CreateWorkspace(out var document);
        var pendingAttachments = new List<ChatAttachment>
        {
            new()
            {
                Kind = AttachmentKind.Page,
                DocumentId = document.Id,
                Title = "guide.pdf · 第 2 页",
                StartPage = 2,
                EndPage = 2
            }
        };
        var chatUpdates = new List<ChatConversation>();
        var chatService = new TestAiChatService("answer from model");
        var host = CreateHost(
            workspace,
            document,
            chatService,
            pendingAttachmentsProvider: () => pendingAttachments.ToArray(),
            clearAttachments: pendingAttachments.Clear,
            chatResultSink: (_, conversation) => chatUpdates.Add(conversation));

        var pending = await host.ExecuteAsync("chat ask current \"explain attached page\"", "test-agent");

        Assert.Empty(document.Conversations);
        Assert.Single(pendingAttachments);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Contains("configured chat model", pendingAudit.Preview.Summary);
        Assert.Contains(document.Id, Assert.Single(pendingAudit.Preview.Targets));
        Assert.Contains("prompt: explain attached page", pendingAudit.Preview.Details);
        Assert.Contains("queuedAttachments: 1", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync("chat ask current \"explain attached page\"", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);
        Assert.Contains("answer from model", approved.Stdout);
        Assert.Empty(pendingAttachments);

        var conversation = Assert.Single(document.Conversations);
        Assert.Same(conversation, Assert.Single(chatUpdates));
        Assert.Equal(2, conversation.Messages.Count);
        Assert.Equal(ChatRole.User, conversation.Messages[0].Role);
        Assert.Equal("MSP", conversation.Messages[0].Author);
        Assert.Equal("explain attached page", conversation.Messages[0].Content);
        Assert.Equal(2, Assert.Single(conversation.Messages[0].Attachments).StartPage);
        Assert.Equal(ChatRole.Assistant, conversation.Messages[1].Role);
        Assert.Equal("answer from model", conversation.Messages[1].Content);
        Assert.Equal("explain attached page", chatService.LastPrompt);
        Assert.Single(chatService.LastAttachments);
    }

    [Fact]
    public async Task Chat_ask_artifact_requires_approval_and_persists_attachment_provenance()
    {
        var workspace = CreateWorkspace(out var document);
        var pendingAttachments = new List<ChatAttachment>
        {
            new()
            {
                Kind = AttachmentKind.PageRange,
                DocumentId = document.Id,
                Title = "guide.pdf · pages 2-4",
                StartPage = 2,
                EndPage = 4
            }
        };
        var chatUpdates = new List<ChatConversation>();
        var chatService = new TestAiChatService("artifact answer");
        var host = CreateHost(
            workspace,
            document,
            chatService,
            pendingAttachmentsProvider: () => pendingAttachments.ToArray(),
            clearAttachments: pendingAttachments.Clear,
            chatResultSink: (_, conversation) => chatUpdates.Add(conversation));
        const string commandText = "chat ask current \"write notes\" --artifact /artifacts/chat/notes.md";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.CreateArtifact |
            MspCommandEffects.ExternalModel,
            pendingAudit.Effects);
        Assert.Contains("/artifacts/chat/notes.md", pendingAudit.Preview.Targets);
        Assert.Contains("queuedAttachments: 1", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("artifact\t/artifacts/chat/notes.md", approved.Stdout);
        Assert.Empty(pendingAttachments);
        Assert.Single(chatUpdates);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/chat/notes.md", resultArtifact.Path);
        Assert.Equal("text/markdown", resultArtifact.MediaType);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:2-4", Assert.Single(resultArtifact.SourcePages));
        Assert.Equal(new[]
        {
            $"/documents/{document.Id}/pages/2.txt",
            $"/documents/{document.Id}/pages/3.txt",
            $"/documents/{document.Id}/pages/4.txt"
        }, resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/chat/notes.md", persisted.Path);
        Assert.Contains("# MSP Chat Answer", persisted.Content);
        Assert.Contains("Question: write notes", persisted.Content);
        Assert.Contains("artifact answer", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal("reados-workbench", persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:2-4", Assert.Single(persisted.SourcePages));
        Assert.Contains("attachments: 1", persisted.Preview);
    }

    [Fact]
    public async Task Chat_ask_artifact_validates_artifact_path_before_model_call()
    {
        var workspace = CreateWorkspace(out var document);
        var chatService = new TestAiChatService("unused answer");
        var host = CreateHost(workspace, document, chatService);

        var result = await host.ExecuteApprovedAsync("chat ask current \"write notes\" --artifact /artifacts", "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.Contains("under /artifacts", result.Stderr);
        Assert.Null(chatService.LastPrompt);
        Assert.Empty(document.Conversations);
        Assert.Empty(workspace.Artifacts);
    }

    [Fact]
    public async Task Chat_ask_returns_recovery_diagnostic_for_model_provider_failure()
    {
        var workspace = CreateWorkspace(out var document);
        var pendingAttachments = new List<ChatAttachment>
        {
            new()
            {
                Kind = AttachmentKind.Page,
                DocumentId = document.Id,
                Title = "guide.pdf · page 2",
                StartPage = 2,
                EndPage = 2
            }
        };
        var chatUpdates = new List<ChatConversation>();
        var chatService = new TestAiChatService(new AiChatServiceException("upstream provider returned 500"));
        var host = CreateHost(
            workspace,
            document,
            chatService,
            pendingAttachmentsProvider: () => pendingAttachments.ToArray(),
            clearAttachments: pendingAttachments.Clear,
            chatResultSink: (_, conversation) => chatUpdates.Add(conversation));

        var result = await host.ExecuteApprovedAsync(
            "chat ask current \"write notes\" --artifact /artifacts/chat/notes.md",
            "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal(
            "Chat model provider failed. Provider response details were withheld to protect request and credential data.",
            result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.chat.model_provider_failed", diagnostic.Code);
        Assert.Equal("OpenAI Compatible/gpt-4.1-mini", diagnostic.Target);
        Assert.Contains("provider base URL", diagnostic.RecoveryHint);

        Assert.Single(pendingAttachments);
        Assert.Empty(document.Conversations);
        Assert.Empty(chatUpdates);
        Assert.Empty(workspace.Artifacts);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.chat.model_provider_failed", auditDiagnostic.Code);
        Assert.Equal("OpenAI Compatible/gpt-4.1-mini", auditDiagnostic.Target);
    }

    [Fact]
    public async Task Chat_ask_streams_progress_events_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        var chatService = new TestAiChatService("streamed answer");
        var host = CreateHost(workspace, document, chatService);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in host.ExecuteApprovedStreamingAsync(
            "chat ask current \"summarize progress\"",
            "test-agent"))
        {
            events.Add(commandEvent);
        }

        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.PolicyDecision &&
            item.Decision == MspPolicyDecision.Allow);
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.Message.Contains("Preparing chat request", StringComparison.Ordinal));
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.Message.Contains("Calling chat model", StringComparison.Ordinal));
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.Message.Contains("Chat answer saved", StringComparison.Ordinal));
        var completed = events[^1];
        Assert.Equal(MspCommandEventKind.Completed, completed.Kind);
        Assert.Equal(0, completed.ExitCode);
        Assert.Equal("reados-workbench", completed.SessionId);
        Assert.Single(document.Conversations);
    }

    [Fact]
    public async Task Artifact_write_persists_provenance_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        var host = CreateHost(workspace, document);
        const string commandText = "artifact write /artifacts/notes.md \"agent notes\"";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Contains("/artifacts/notes.md", Assert.Single(pendingAudit.Preview.Targets));

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/notes.md", resultArtifact.Path);
        Assert.Equal("test-agent", resultArtifact.Actor);
        Assert.Equal("reados-workbench", resultArtifact.SessionId);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/notes.md", persisted.Path);
        Assert.Equal("agent notes", persisted.Content);
        Assert.Equal("text/markdown", persisted.MediaType);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal("reados-workbench", persisted.SessionId);
        Assert.Equal("contentLength: 11", persisted.Preview);
    }

    [Fact]
    public async Task Pdf_text_artifact_requires_approval_and_persists_source_provenance()
    {
        var workspace = CreateWorkspace(out var document);
        var pdfService = new TestPdfDocumentService
        {
            ExtractedText = "page 2 text\npage 3 text"
        };
        var host = CreateHost(workspace, document, pdfService: pdfService);
        const string commandText = "pdf text current 2 3 --artifact /artifacts/excerpts/guide-pages.txt";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, pendingAudit.Effects);
        Assert.Contains("/artifacts/excerpts/guide-pages.txt", pendingAudit.Preview.Targets);
        Assert.Contains(pendingAudit.Preview.Targets, target => target.Contains(document.Id, StringComparison.Ordinal));
        Assert.Contains("pages: 2-3", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("artifact\t/artifacts/excerpts/guide-pages.txt\t23", approved.Stdout);
        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/excerpts/guide-pages.txt", resultArtifact.Path);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:2-3", Assert.Single(resultArtifact.SourcePages));
        Assert.Equal(new[]
        {
            $"/documents/{document.Id}/pages/2.txt",
            $"/documents/{document.Id}/pages/3.txt"
        }, resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/excerpts/guide-pages.txt", persisted.Path);
        Assert.Equal("page 2 text\npage 3 text", persisted.Content);
        Assert.Equal("text/plain", persisted.MediaType);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal("reados-workbench", persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:2-3", Assert.Single(persisted.SourcePages));
        Assert.Contains("contentLength: 23", persisted.Preview);
    }

    [Fact]
    public async Task Pdf_commands_return_recovery_diagnostic_for_missing_document()
    {
        var workspace = CreateWorkspace(out var document);
        var host = CreateHost(workspace, document);

        var result = await host.ExecuteAsync("pdf inspect missing-doc", "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal("PDF document not found: missing-doc", result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.pdf.document_not_found", diagnostic.Code);
        Assert.Equal("missing-doc", diagnostic.Target);
        Assert.Contains("library list", diagnostic.RecoveryHint);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.pdf.document_not_found", auditDiagnostic.Code);
        Assert.Equal("missing-doc", auditDiagnostic.Target);
    }

    [Fact]
    public async Task Pdf_metadata_commands_return_recovery_diagnostic_for_invalid_page()
    {
        var workspace = CreateWorkspace(out var document);
        var host = CreateHost(workspace, document);

        var result = await host.ExecuteApprovedAsync("page-label set current 99 appendix", "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.Equal($"PDF page is outside document range: 99 (1-{document.PageCount})", result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.pdf.invalid_page", diagnostic.Code);
        Assert.Equal($"{document.Id}:99", diagnostic.Target);
        Assert.Contains("pdf inspect current", diagnostic.RecoveryHint);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.pdf.invalid_page", auditDiagnostic.Code);
        Assert.Equal($"{document.Id}:99", auditDiagnostic.Target);
    }

    [Fact]
    public async Task Outline_delete_returns_recovery_diagnostic_for_missing_outline_item()
    {
        var workspace = CreateWorkspace(out var document);
        var host = CreateHost(workspace, document);

        var result = await host.ExecuteApprovedAsync("outline delete current missing-heading", "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal("Outline item not found: missing-heading", result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.pdf.outline_item_not_found", diagnostic.Code);
        Assert.Equal($"{document.Id}:missing-heading", diagnostic.Target);
        Assert.Contains("pdf inspect current", diagnostic.RecoveryHint);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.pdf.outline_item_not_found", auditDiagnostic.Code);
        Assert.Equal($"{document.Id}:missing-heading", auditDiagnostic.Target);
    }

    [Fact]
    public async Task Pdf_search_artifact_requires_approval_and_persists_hit_page_provenance()
    {
        var workspace = CreateWorkspace(out var document);
        var pdfService = new TestPdfDocumentService
        {
            SearchHits = new[]
            {
                new PdfTextHit(2, "alpha hit"),
                new PdfTextHit(5, "beta hit"),
                new PdfTextHit(2, "second alpha hit")
            }
        };
        var host = CreateHost(workspace, document, pdfService: pdfService);
        const string commandText = "pdf search current alpha --artifact /artifacts/search/alpha.tsv";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, pendingAudit.Effects);
        Assert.Contains("/artifacts/search/alpha.tsv", pendingAudit.Preview.Targets);
        Assert.Contains(pendingAudit.Preview.Targets, target => target.Contains(document.Id, StringComparison.Ordinal));
        Assert.Contains("query: alpha", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("artifact\t/artifacts/search/alpha.tsv\t3", approved.Stdout);
        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("text/tab-separated-values", resultArtifact.MediaType);
        Assert.Equal(new[]
        {
            $"/documents/{document.Id}/pages/2.txt",
            $"/documents/{document.Id}/pages/5.txt"
        }, resultArtifact.SourcePaths);
        Assert.Equal(new[]
        {
            $"{document.Id}:2",
            $"{document.Id}:5"
        }, resultArtifact.SourcePages);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/search/alpha.tsv", persisted.Path);
        Assert.Contains("2\talpha hit", persisted.Content);
        Assert.Contains("5\tbeta hit", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal("reados-workbench", persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal(new[] { $"{document.Id}:2", $"{document.Id}:5" }, persisted.SourcePages);
        Assert.Contains("hits: 3", persisted.Preview);
    }

    [Fact]
    public async Task Workflow_run_explain_section_requires_approval_and_persists_section_artifact()
    {
        var workspace = CreateWorkspace(out var document);
        document.Outline.Add(new OutlineItem { Id = "chapter-3", Title = "3 Architecture", Page = 2, Level = 1 });
        document.Outline.Add(new OutlineItem { Id = "section-3-2", Title = "3.2 Service Layer", Page = 4, Level = 2 });
        document.Outline.Add(new OutlineItem { Id = "section-3-3", Title = "3.3 Agent Bridge", Page = 7, Level = 2 });
        document.Outline.Add(new OutlineItem { Id = "chapter-4", Title = "4 Native Core", Page = 10, Level = 1 });
        var pdfService = new TestPdfDocumentService
        {
            ExtractedText = "section source text"
        };
        var chatService = new TestAiChatService("section explanation");
        var host = CreateHost(workspace, document, chatService, pdfService);
        const string commandText = "workflow run explain-section --document current --outline \"3.2 Service Layer\" --artifact /artifacts/workflows/explain-section.md";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        Assert.Null(chatService.LastPrompt);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.CreateArtifact |
            MspCommandEffects.ExternalModel,
            pendingAudit.Effects);
        Assert.Contains(pendingAudit.Preview.Targets, target => target.Contains(document.Id, StringComparison.Ordinal));
        Assert.Contains("/artifacts/workflows/explain-section.md", pendingAudit.Preview.Targets);
        Assert.Contains("section: 3.2 Service Layer", pendingAudit.Preview.Details);
        Assert.Contains("pages: 4-6", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("# MSP Section Explanation", approved.Stdout);
        Assert.Contains("section explanation", approved.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/explain-section.md", approved.Stdout);
        Assert.Equal(4, pdfService.LastStartPage);
        Assert.Equal(6, pdfService.LastEndPage);
        Assert.NotNull(chatService.LastPrompt);
        Assert.Contains("3.2 Service Layer", chatService.LastPrompt);
        var attachment = Assert.Single(chatService.LastAttachments);
        Assert.Equal(AttachmentKind.PageRange, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal(4, attachment.StartPage);
        Assert.Equal(6, attachment.EndPage);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/workflows/explain-section.md", resultArtifact.Path);
        Assert.Equal("text/markdown", resultArtifact.MediaType);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(resultArtifact.SourcePages));
        Assert.Equal(new[]
        {
            $"/documents/{document.Id}/pages/4.txt",
            $"/documents/{document.Id}/pages/5.txt",
            $"/documents/{document.Id}/pages/6.txt"
        }, resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/workflows/explain-section.md", persisted.Path);
        Assert.Contains("# MSP Section Explanation", persisted.Content);
        Assert.Contains("3.2 Service Layer", persisted.Content);
        Assert.Contains("section explanation", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(persisted.SourcePages));
        Assert.Contains("pages: 4-6", persisted.Preview);
    }

    [Fact]
    public async Task Workflow_run_explain_section_returns_recovery_diagnostic_for_missing_outline()
    {
        var workspace = CreateWorkspace(out var document);
        document.Outline.Add(new OutlineItem { Id = "chapter-3", Title = "3 Architecture", Page = 2, Level = 1 });
        var chatService = new TestAiChatService("unused");
        var host = CreateHost(workspace, document, chatService);

        var result = await host.ExecuteApprovedAsync(
            "workflow run explain-section --outline missing-heading --artifact /artifacts/workflows/missing.md",
            "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal("Outline item not found: missing-heading", result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.pdf.outline_item_not_found", diagnostic.Code);
        Assert.Equal($"{document.Id}:missing-heading", diagnostic.Target);
        Assert.Contains("pdf inspect current", diagnostic.RecoveryHint);
        Assert.Null(chatService.LastPrompt);
        Assert.Empty(workspace.Artifacts);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.pdf.outline_item_not_found", auditDiagnostic.Code);
    }

    [Fact]
    public async Task Workflow_run_extract_evidence_requires_approval_and_persists_structured_artifact()
    {
        var workspace = CreateWorkspace(out var document);
        document.Outline.Add(new OutlineItem { Id = "chapter-3", Title = "3 Architecture", Page = 2, Level = 1 });
        document.Outline.Add(new OutlineItem { Id = "section-3-2", Title = "3.2 Service Layer", Page = 4, Level = 2 });
        document.Outline.Add(new OutlineItem { Id = "section-3-3", Title = "3.3 Agent Bridge", Page = 7, Level = 2 });
        var pdfService = new TestPdfDocumentService();
        pdfService.ExtractedPageText[4] = "page four evidence";
        pdfService.ExtractedPageText[5] = "page five evidence";
        pdfService.ExtractedPageText[6] = "page six evidence";
        var chatService = new TestAiChatService("unused");
        var host = CreateHost(workspace, document, chatService, pdfService);
        const string commandText = "workflow run extract-evidence --document current --outline \"3.2 Service Layer\" --artifact /artifacts/workflows/evidence.json";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        Assert.Null(chatService.LastPrompt);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.CreateArtifact,
            pendingAudit.Effects);
        Assert.Contains(pendingAudit.Preview.Targets, target => target.Contains(document.Id, StringComparison.Ordinal));
        Assert.Contains("/artifacts/workflows/evidence.json", pendingAudit.Preview.Targets);
        Assert.Contains("workflow: extract-evidence", pendingAudit.Preview.Details);
        Assert.Contains("section: 3.2 Service Layer", pendingAudit.Preview.Details);
        Assert.Contains("pages: 4-6", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("\"workflow\": \"extract-evidence\"", approved.Stdout);
        Assert.Contains("\"sourcePath\": \"/documents/", approved.Stdout);
        Assert.Contains("page four evidence", approved.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/evidence.json\t3", approved.Stdout);
        Assert.Equal(new[]
        {
            (4, 4),
            (5, 5),
            (6, 6)
        }, pdfService.ExtractCalls);
        Assert.Null(chatService.LastPrompt);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/workflows/evidence.json", resultArtifact.Path);
        Assert.Equal("application/json", resultArtifact.MediaType);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(resultArtifact.SourcePages));
        Assert.Equal(new[]
        {
            $"/documents/{document.Id}/pages/4.txt",
            $"/documents/{document.Id}/pages/5.txt",
            $"/documents/{document.Id}/pages/6.txt"
        }, resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/workflows/evidence.json", persisted.Path);
        Assert.Equal("application/json", persisted.MediaType);
        Assert.Contains("\"title\": \"3.2 Service Layer\"", persisted.Content);
        Assert.Contains("page six evidence", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(persisted.SourcePages));
        Assert.Contains("entries: 3", persisted.Preview);
    }

    [Fact]
    public async Task Workflow_run_extract_evidence_validates_artifact_path_before_extracting_pages()
    {
        var workspace = CreateWorkspace(out var document);
        document.Outline.Add(new OutlineItem { Id = "section-3-2", Title = "3.2 Service Layer", Page = 4, Level = 2 });
        var pdfService = new TestPdfDocumentService();
        var host = CreateHost(workspace, document, pdfService: pdfService);

        var result = await host.ExecuteApprovedAsync(
            "workflow run extract-evidence --outline \"3.2 Service Layer\" --artifact /artifacts",
            "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.Equal("workflow run extract-evidence --artifact must target a file under /artifacts.", result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.workflow.invalid_artifact_path", diagnostic.Code);
        Assert.Equal("/artifacts", diagnostic.Target);
        Assert.Contains("/artifacts/workflows/evidence.json", diagnostic.RecoveryHint);
        Assert.Empty(pdfService.ExtractCalls);
        Assert.Empty(workspace.Artifacts);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.workflow.invalid_artifact_path", auditDiagnostic.Code);
    }

    [Fact]
    public async Task Workflow_run_review_evidence_requires_approval_and_persists_review_with_inherited_provenance()
    {
        var workspace = CreateWorkspace(out var document);
        var evidenceArtifact = new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence.json",
            Content = $$"""
            {
              "workflow": "extract-evidence",
              "generatedAt": "2026-07-07T00:00:00+00:00",
              "document": {
                "id": "{{document.Id}}",
                "name": "guide.pdf"
              },
              "section": {
                "id": "section-3-2",
                "title": "3.2 Service Layer",
                "level": 2,
                "startPage": 4,
                "endPage": 6
              },
              "pages": [
                {
                  "page": 4,
                  "sourcePath": "/documents/{{document.Id}}/pages/4.txt",
                  "textLength": 18,
                  "text": "page four evidence"
                },
                {
                  "page": 5,
                  "sourcePath": "/documents/{{document.Id}}/pages/5.txt",
                  "textLength": 18,
                  "text": "page five evidence"
                },
                {
                  "page": 6,
                  "sourcePath": "/documents/{{document.Id}}/pages/6.txt",
                  "textLength": 17,
                  "text": "page six evidence"
                }
              ]
            }
            """,
            MediaType = "application/json",
            SourceCommand = "workflow run extract-evidence --document current --outline \"3.2 Service Layer\" --artifact /artifacts/workflows/evidence.json",
            Actor = "test-agent",
            SessionId = ReadOsMspHost.DefaultSessionId,
            Preview = "document: guide.pdf; section: 3.2 Service Layer; pages: 4-6; entries: 3"
        };
        evidenceArtifact.SourceDocuments.Add(document.Id);
        evidenceArtifact.SourcePages.Add($"{document.Id}:4-6");
        evidenceArtifact.SourcePaths.Add($"/documents/{document.Id}/pages/4.txt");
        evidenceArtifact.SourcePaths.Add($"/documents/{document.Id}/pages/5.txt");
        evidenceArtifact.SourcePaths.Add($"/documents/{document.Id}/pages/6.txt");
        workspace.Artifacts.Add(evidenceArtifact);
        var pdfService = new TestPdfDocumentService();
        var chatService = new TestAiChatService("unused");
        var host = CreateHost(workspace, document, chatService, pdfService);
        const string commandText = "workflow run review-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-review.md";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Single(workspace.Artifacts);
        Assert.Null(chatService.LastPrompt);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.CreateArtifact,
            pendingAudit.Effects);
        Assert.Contains("/artifacts/workflows/evidence.json", pendingAudit.Preview.Targets);
        Assert.Contains("/artifacts/workflows/evidence-review.md", pendingAudit.Preview.Targets);
        Assert.Contains("workflow: review-evidence", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("# MSP Evidence Review", approved.Stdout);
        Assert.Contains("## Citation Table", approved.Stdout);
        Assert.Contains("page four evidence", approved.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/evidence-review.md", approved.Stdout);
        Assert.Empty(pdfService.ExtractCalls);
        Assert.Null(chatService.LastPrompt);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/workflows/evidence-review.md", resultArtifact.Path);
        Assert.Equal("text/markdown", resultArtifact.MediaType);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(resultArtifact.SourcePages));
        Assert.Contains("/artifacts/workflows/evidence.json", resultArtifact.SourcePaths);
        Assert.Contains("/artifacts/workflows/evidence.json.manifest.json", resultArtifact.SourcePaths);
        Assert.Contains($"/documents/{document.Id}/pages/4.txt", resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts, item => item.Path == "/artifacts/workflows/evidence-review.md");
        Assert.Equal("text/markdown", persisted.MediaType);
        Assert.Contains("# MSP Evidence Review", persisted.Content);
        Assert.Contains("3.2 Service Layer", persisted.Content);
        Assert.Contains("page six evidence", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(persisted.SourcePages));
        Assert.Contains("/artifacts/workflows/evidence.json", persisted.SourcePaths);
        Assert.Contains($"/documents/{document.Id}/pages/6.txt", persisted.SourcePaths);
        Assert.Contains("citations: 3", persisted.Preview);
    }

    [Fact]
    public async Task Workflow_run_review_evidence_returns_recovery_diagnostic_for_missing_evidence_artifact()
    {
        var workspace = CreateWorkspace(out var document);
        var host = CreateHost(workspace, document);

        var result = await host.ExecuteApprovedAsync(
            "workflow run review-evidence --evidence /artifacts/workflows/missing.json --artifact /artifacts/workflows/review.md",
            "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal("Evidence artifact not found: /artifacts/workflows/missing.json", result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.workflow.evidence_artifact_not_found", diagnostic.Code);
        Assert.Equal("/artifacts/workflows/missing.json", diagnostic.Target);
        Assert.Contains("extract-evidence", diagnostic.RecoveryHint);
        Assert.Empty(workspace.Artifacts);

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.workflow.evidence_artifact_not_found", auditDiagnostic.Code);
    }

    [Fact]
    public async Task Workflow_run_synthesize_evidence_requires_approval_and_persists_model_synthesis()
    {
        var workspace = CreateWorkspace(out var document);
        AddEvidenceArtifact(workspace, document);
        var pdfService = new TestPdfDocumentService();
        var chatService = new TestAiChatService("model synthesis from evidence");
        var host = CreateHost(workspace, document, chatService, pdfService);
        const string commandText = "workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Single(workspace.Artifacts);
        Assert.Null(chatService.LastPrompt);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.CreateArtifact |
            MspCommandEffects.ExternalModel,
            pendingAudit.Effects);
        Assert.Contains("/artifacts/workflows/evidence.json", pendingAudit.Preview.Targets);
        Assert.Contains("/artifacts/workflows/evidence-synthesis.md", pendingAudit.Preview.Targets);
        Assert.Contains("workflow: synthesize-evidence", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("# MSP Evidence Synthesis", approved.Stdout);
        Assert.Contains("model synthesis from evidence", approved.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/evidence-synthesis.md", approved.Stdout);
        Assert.Empty(pdfService.ExtractCalls);
        Assert.NotNull(chatService.LastPrompt);
        Assert.Contains("Synthesize the structured ReadOS evidence", chatService.LastPrompt);
        Assert.Contains("3.2 Service Layer", chatService.LastPrompt);
        var attachment = Assert.Single(chatService.LastAttachments);
        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal("/artifacts/workflows/evidence.json", attachment.FilePath);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/workflows/evidence-synthesis.md", resultArtifact.Path);
        Assert.Equal("text/markdown", resultArtifact.MediaType);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(resultArtifact.SourcePages));
        Assert.Contains("/artifacts/workflows/evidence.json", resultArtifact.SourcePaths);
        Assert.Contains("/artifacts/workflows/evidence.json.manifest.json", resultArtifact.SourcePaths);
        Assert.Contains($"/documents/{document.Id}/pages/5.txt", resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts, item => item.Path == "/artifacts/workflows/evidence-synthesis.md");
        Assert.Equal("text/markdown", persisted.MediaType);
        Assert.Contains("# MSP Evidence Synthesis", persisted.Content);
        Assert.Contains("model synthesis from evidence", persisted.Content);
        Assert.Contains("## Source Pages", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(persisted.SourcePages));
        Assert.Contains("/artifacts/workflows/evidence.json", persisted.SourcePaths);
        Assert.Contains($"/documents/{document.Id}/pages/6.txt", persisted.SourcePaths);
        Assert.Contains("citations: 3", persisted.Preview);
    }

    [Fact]
    public async Task Workflow_run_synthesize_evidence_returns_recovery_diagnostic_for_model_provider_failure()
    {
        var workspace = CreateWorkspace(out var document);
        AddEvidenceArtifact(workspace, document);
        var pdfService = new TestPdfDocumentService();
        var chatService = new TestAiChatService(new AiChatServiceException("provider unavailable"));
        var host = CreateHost(workspace, document, chatService, pdfService);

        var result = await host.ExecuteApprovedAsync(
            "workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md",
            "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal(
            "Chat model provider failed. Provider response details were withheld to protect request and credential data.",
            result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.chat.model_provider_failed", diagnostic.Code);
        Assert.Contains("provider base URL", diagnostic.RecoveryHint);
        Assert.Empty(pdfService.ExtractCalls);
        Assert.NotNull(chatService.LastPrompt);
        Assert.Single(workspace.Artifacts);
        Assert.DoesNotContain(workspace.Artifacts, item => item.Path == "/artifacts/workflows/evidence-synthesis.md");

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.chat.model_provider_failed", auditDiagnostic.Code);
    }

    [Fact]
    public async Task Workflow_run_refine_artifact_requires_approval_and_persists_refined_derivative()
    {
        var workspace = CreateWorkspace(out var document);
        AddSynthesisArtifact(workspace, document);
        var pdfService = new TestPdfDocumentService();
        var chatService = new TestAiChatService("refined synthesis with tighter caveats");
        var host = CreateHost(workspace, document, chatService, pdfService);
        const string commandText = "workflow run refine-artifact --source /artifacts/workflows/evidence-synthesis.md --instruction \"tighten caveats and keep citations\" --artifact /artifacts/workflows/evidence-synthesis-refined.md";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Single(workspace.Artifacts);
        Assert.Null(chatService.LastPrompt);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.CreateArtifact |
            MspCommandEffects.ExternalModel,
            pendingAudit.Effects);
        Assert.Contains("/artifacts/workflows/evidence-synthesis.md", pendingAudit.Preview.Targets);
        Assert.Contains("/artifacts/workflows/evidence-synthesis-refined.md", pendingAudit.Preview.Targets);
        Assert.Contains("workflow: refine-artifact", pendingAudit.Preview.Details);
        Assert.Contains("instruction: tighten caveats and keep citations", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("# MSP Artifact Refinement", approved.Stdout);
        Assert.Contains("refined synthesis with tighter caveats", approved.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/evidence-synthesis-refined.md", approved.Stdout);
        Assert.Empty(pdfService.ExtractCalls);
        Assert.NotNull(chatService.LastPrompt);
        Assert.Contains("tighten caveats and keep citations", chatService.LastPrompt);
        var attachment = Assert.Single(chatService.LastAttachments);
        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal("/artifacts/workflows/evidence-synthesis.md", attachment.FilePath);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/workflows/evidence-synthesis-refined.md", resultArtifact.Path);
        Assert.Equal("text/markdown", resultArtifact.MediaType);
        Assert.Equal(document.Id, Assert.Single(resultArtifact.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(resultArtifact.SourcePages));
        Assert.Contains("/artifacts/workflows/evidence-synthesis.md", resultArtifact.SourcePaths);
        Assert.Contains("/artifacts/workflows/evidence-synthesis.md.manifest.json", resultArtifact.SourcePaths);
        Assert.Contains($"/documents/{document.Id}/pages/4.txt", resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts, item => item.Path == "/artifacts/workflows/evidence-synthesis-refined.md");
        Assert.Equal("text/markdown", persisted.MediaType);
        Assert.Contains("# MSP Artifact Refinement", persisted.Content);
        Assert.Contains("refined synthesis with tighter caveats", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, persisted.SessionId);
        Assert.Equal(document.Id, Assert.Single(persisted.SourceDocuments));
        Assert.Equal($"{document.Id}:4-6", Assert.Single(persisted.SourcePages));
        Assert.Contains("/artifacts/workflows/evidence-synthesis.md", persisted.SourcePaths);
        Assert.Contains($"/documents/{document.Id}/pages/6.txt", persisted.SourcePaths);
        Assert.Contains("tighten caveats", persisted.Preview);
    }

    [Fact]
    public async Task Workflow_run_refine_artifact_returns_recovery_diagnostic_for_model_provider_failure()
    {
        var workspace = CreateWorkspace(out var document);
        AddSynthesisArtifact(workspace, document);
        var chatService = new TestAiChatService(new AiChatServiceException("provider unavailable"));
        var host = CreateHost(workspace, document, chatService);

        var result = await host.ExecuteApprovedAsync(
            "workflow run refine-artifact --source /artifacts/workflows/evidence-synthesis.md --instruction \"tighten caveats\" --artifact /artifacts/workflows/evidence-synthesis-refined.md",
            "test-agent");

        Assert.False(result.Succeeded);
        Assert.Equal(
            "Chat model provider failed. Provider response details were withheld to protect request and credential data.",
            result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal("reados.chat.model_provider_failed", diagnostic.Code);
        Assert.Contains("provider base URL", diagnostic.RecoveryHint);
        Assert.NotNull(chatService.LastPrompt);
        Assert.Single(workspace.Artifacts);
        Assert.DoesNotContain(workspace.Artifacts, item => item.Path == "/artifacts/workflows/evidence-synthesis-refined.md");

        var audit = Assert.Single(result.AuditRecords);
        var auditDiagnostic = Assert.Single(audit.Diagnostics);
        Assert.Equal("reados.chat.model_provider_failed", auditDiagnostic.Code);
    }

    [Fact]
    public async Task Workflow_summary_artifact_requires_approval_and_persists_session_provenance()
    {
        var workspace = CreateWorkspace(out var document);
        var success = new MspTranscriptEntry
        {
            Id = "transcript-ok",
            Actor = "test-agent",
            SessionId = ReadOsMspHost.DefaultSessionId,
            CommandText = "pdf search current alpha --artifact /artifacts/search/alpha.tsv",
            ExitCode = 0,
            Stdout = "artifact\t/artifacts/search/alpha.tsv\t3",
            Decision = "Allow",
            Effects = "ReadWorkspace, WriteWorkspace, CreateArtifact",
            ArtifactsSummary = "/artifacts/search/alpha.tsv"
        };
        var failure = new MspTranscriptEntry
        {
            Id = "transcript-fail",
            Actor = "test-agent",
            SessionId = ReadOsMspHost.DefaultSessionId,
            CommandText = "missing-command",
            ExitCode = 127,
            Stderr = "Command not found: missing-command",
            Decision = "Allow",
            Effects = "None",
            DiagnosticsSummary = "error msp.command_not_found: Command not found: missing-command",
            RecoveryHint = "Run help to list available MSP commands."
        };
        workspace.MspTranscript.Add(success);
        workspace.MspTranscript.Add(failure);
        var session = new MspSessionEntry
        {
            Id = ReadOsMspHost.DefaultSessionId,
            Title = "ReadOS Workbench MSP Session",
            Actor = "test-agent",
            LastCommandText = failure.CommandText,
            LastExitCode = failure.ExitCode,
            CommandCount = 2,
            ApprovalCount = 1,
            FailureCount = 1,
            LastDiagnosticsSummary = failure.DiagnosticsSummary,
            LastRecoveryHint = failure.RecoveryHint
        };
        session.TranscriptIds.Add(success.Id);
        session.TranscriptIds.Add(failure.Id);
        session.ArtifactPaths.Add("/artifacts/search/alpha.tsv");
        workspace.MspSessions.Add(session);
        var host = CreateHost(workspace, document);
        const string commandText = "workflow summary current --artifact /artifacts/workflows/current.md";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Equal(
            MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
            pendingAudit.Effects);
        Assert.Contains("/artifacts/workflows/current.md", pendingAudit.Preview.Targets);

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Contains("# MSP Workflow Summary", approved.Stdout);
        Assert.Contains("artifact\t/artifacts/workflows/current.md", approved.Stdout);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/workflows/current.md", resultArtifact.Path);
        Assert.Equal("text/markdown", resultArtifact.MediaType);
        Assert.Contains("/sessions/reados-workbench.json", resultArtifact.SourcePaths);
        Assert.Contains("/transcripts/transcript-ok.json", resultArtifact.SourcePaths);
        Assert.Contains("/transcripts/transcript-fail.json", resultArtifact.SourcePaths);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/workflows/current.md", persisted.Path);
        Assert.Contains("ReadOS Workbench MSP Session", persisted.Content);
        Assert.Contains("missing-command", persisted.Content);
        Assert.Contains("msp.command_not_found", persisted.Content);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal(ReadOsMspHost.DefaultSessionId, persisted.SessionId);
        Assert.Contains("/sessions/reados-workbench.json", persisted.SourcePaths);
        Assert.Contains("/transcripts/transcript-fail.json", persisted.SourcePaths);
    }

    private static ReadOsMspHost CreateHost(
        WorkspaceState workspace,
        LibraryItem document,
        IAiChatService? chatService = null,
        IPdfDocumentService? pdfService = null,
        Func<IReadOnlyList<ChatAttachment>>? pendingAttachmentsProvider = null,
        Action<ChatAttachment>? attachmentSink = null,
        Action? clearAttachments = null,
        Action<LibraryItem, ChatConversation>? chatResultSink = null)
    {
        return new ReadOsMspHost(CreateHostDependencies(
            workspace,
            document,
            chatService,
            pdfService,
            pendingAttachmentsProvider,
            attachmentSink,
            clearAttachments,
            chatResultSink));
    }

    private static ReadOsMspHostDependencies CreateHostDependencies(
        WorkspaceState workspace,
        LibraryItem document,
        IAiChatService? chatService = null,
        IPdfDocumentService? pdfService = null,
        Func<IReadOnlyList<ChatAttachment>>? pendingAttachmentsProvider = null,
        Action<ChatAttachment>? attachmentSink = null,
        Action? clearAttachments = null,
        Action<LibraryItem, ChatConversation>? chatResultSink = null,
        MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter = null)
    {
        return new ReadOsMspHostDependencies(
            new TestWorkspaceStore(workspace),
            pdfService ?? new TestPdfDocumentService(),
            chatService ?? new TestAiChatService("unused"),
            () => workspace,
            () => workspace.Settings,
            () => document,
            pendingAttachmentsProvider ?? (() => Array.Empty<ChatAttachment>()),
            _ => Task.FromResult("attachment text"),
            attachmentSink ?? (_ => { }),
            clearAttachments ?? (() => { }),
            chatResultSink ?? ((_, _) => { }),
            runtimeFfiEchoCommandAdapter);
    }

    private static ReadOsMspCommandPackFactory CreateCommandPackFactory(
        WorkspaceState workspace,
        LibraryItem document,
        IAiChatService? chatService = null,
        IPdfDocumentService? pdfService = null,
        Func<IReadOnlyList<ChatAttachment>>? pendingAttachmentsProvider = null,
        Action<ChatAttachment>? attachmentSink = null,
        Action? clearAttachments = null,
        Action<LibraryItem, ChatConversation>? chatResultSink = null)
    {
        return new ReadOsMspCommandPackFactory(
            new TestWorkspaceStore(workspace),
            pdfService ?? new TestPdfDocumentService(),
            chatService ?? new TestAiChatService("unused"),
            () => workspace,
            () => workspace.Settings,
            () => document,
            pendingAttachmentsProvider ?? (() => Array.Empty<ChatAttachment>()),
            _ => Task.FromResult("attachment text"),
            attachmentSink ?? (_ => { }),
            clearAttachments ?? (() => { }),
            chatResultSink ?? ((_, _) => { }));
    }

    private static WorkspaceState CreateWorkspace(out LibraryItem document)
    {
        var workspace = new WorkspaceState();
        var project = new ProjectItem
        {
            Name = "Project"
        };
        document = new LibraryItem
        {
            ProjectId = project.Id,
            Kind = LibraryItemKind.Pdf,
            Name = "guide.pdf",
            PageCount = 12,
            CurrentPage = 1
        };
        project.LibraryItems.Add(document);
        workspace.Projects.Add(project);
        return workspace;
    }

    private static WorkspaceArtifact AddEvidenceArtifact(WorkspaceState workspace, LibraryItem document)
    {
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence.json",
            Content = $$"""
            {
              "workflow": "extract-evidence",
              "generatedAt": "2026-07-07T00:00:00+00:00",
              "document": {
                "id": "{{document.Id}}",
                "name": "guide.pdf"
              },
              "section": {
                "id": "section-3-2",
                "title": "3.2 Service Layer",
                "level": 2,
                "startPage": 4,
                "endPage": 6
              },
              "pages": [
                {
                  "page": 4,
                  "sourcePath": "/documents/{{document.Id}}/pages/4.txt",
                  "textLength": 18,
                  "text": "page four evidence"
                },
                {
                  "page": 5,
                  "sourcePath": "/documents/{{document.Id}}/pages/5.txt",
                  "textLength": 18,
                  "text": "page five evidence"
                },
                {
                  "page": 6,
                  "sourcePath": "/documents/{{document.Id}}/pages/6.txt",
                  "textLength": 17,
                  "text": "page six evidence"
                }
              ]
            }
            """,
            MediaType = "application/json",
            SourceCommand = "workflow run extract-evidence --document current --outline \"3.2 Service Layer\" --artifact /artifacts/workflows/evidence.json",
            Actor = "test-agent",
            SessionId = ReadOsMspHost.DefaultSessionId,
            Preview = "document: guide.pdf; section: 3.2 Service Layer; pages: 4-6; entries: 3"
        };
        artifact.SourceDocuments.Add(document.Id);
        artifact.SourcePages.Add($"{document.Id}:4-6");
        artifact.SourcePaths.Add($"/documents/{document.Id}/pages/4.txt");
        artifact.SourcePaths.Add($"/documents/{document.Id}/pages/5.txt");
        artifact.SourcePaths.Add($"/documents/{document.Id}/pages/6.txt");
        workspace.Artifacts.Add(artifact);
        return artifact;
    }

    private static WorkspaceArtifact AddSynthesisArtifact(WorkspaceState workspace, LibraryItem document)
    {
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence-synthesis.md",
            Content = """
            # MSP Evidence Synthesis

            ## Synthesis

            The service layer evidence supports a cautious conclusion.
            """,
            MediaType = "text/markdown",
            SourceCommand = "workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md",
            Actor = "test-agent",
            SessionId = ReadOsMspHost.DefaultSessionId,
            Preview = "evidence: /artifacts/workflows/evidence.json; document: guide.pdf; section: 3.2 Service Layer; citations: 3"
        };
        artifact.SourceDocuments.Add(document.Id);
        artifact.SourcePages.Add($"{document.Id}:4-6");
        artifact.SourcePaths.Add("/artifacts/workflows/evidence.json");
        artifact.SourcePaths.Add("/artifacts/workflows/evidence.json.manifest.json");
        artifact.SourcePaths.Add($"/documents/{document.Id}/pages/4.txt");
        artifact.SourcePaths.Add($"/documents/{document.Id}/pages/5.txt");
        artifact.SourcePaths.Add($"/documents/{document.Id}/pages/6.txt");
        workspace.Artifacts.Add(artifact);
        return artifact;
    }

    private sealed class TestWorkspaceStore : IWorkspaceStore
    {
        private readonly WorkspaceState workspace;

        public TestWorkspaceStore(WorkspaceState workspace)
        {
            this.workspace = workspace;
        }

        public string WorkspaceRoot => "V:\\ReadOS-Test";

        public string LibraryRoot => "V:\\ReadOS-Test\\Library";

        public Task<WorkspaceState> LoadAsync(CancellationToken cancellationToken = default)
        {
            return Task.FromResult(workspace);
        }

        public Task SaveAsync(WorkspaceState state, CancellationToken cancellationToken = default)
        {
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

    private sealed class ProtectedNameTestCommand(string name) : IMspCommand
    {
        public string Name { get; } = name;

        public string Summary => "Synthetic protected-name override.";

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspCommandResult.Success());
        }
    }

    private sealed class TrackingNativeAdapterProvider : IMspNativeAdapterProvider
    {
        public bool IsAdapterCreated { get; private set; }

        public int DisposeCalls { get; private set; }

        public IMspNativeAdapter GetRequiredAdapter()
        {
            IsAdapterCreated = true;
            throw new InvalidOperationException(
                "The test did not expect the native adapter to be created.");
        }

        public void Dispose()
        {
            DisposeCalls++;
        }
    }

    private sealed class TestPdfDocumentService : IPdfDocumentService
    {
        public string ExtractedText { get; init; } = string.Empty;

        public IReadOnlyList<PdfTextHit> SearchHits { get; init; } = Array.Empty<PdfTextHit>();

        public Dictionary<int, string> ExtractedPageText { get; } = new();

        public List<(int StartPage, int EndPage)> ExtractCalls { get; } = new();

        public int LastStartPage { get; private set; }

        public int LastEndPage { get; private set; }

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
            return Task.FromResult(SearchHits);
        }

        public Task<string> ExtractPageTextAsync(string path, int startPage, int endPage, CancellationToken cancellationToken = default)
        {
            LastStartPage = startPage;
            LastEndPage = endPage;
            ExtractCalls.Add((startPage, endPage));
            return startPage == endPage && ExtractedPageText.TryGetValue(startPage, out var text)
                ? Task.FromResult(text)
                : Task.FromResult(ExtractedText);
        }
    }

    private sealed class TestAiChatService : IAiChatService
    {
        private readonly string response;
        private readonly Exception? exception;

        public TestAiChatService(string response)
        {
            this.response = response;
        }

        public TestAiChatService(Exception exception)
        {
            response = string.Empty;
            this.exception = exception;
        }

        public string? LastPrompt { get; private set; }

        public IReadOnlyList<ChatAttachment> LastAttachments { get; private set; } = Array.Empty<ChatAttachment>();

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
            LastPrompt = userPrompt;
            LastAttachments = attachments.ToArray();
            if (exception is not null)
            {
                throw exception;
            }

            return Task.FromResult(response);
        }
    }
}

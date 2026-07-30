using System.Text.Json;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Tests.Commands;

public sealed class WorkflowCommandPathSecurityTests
{
    [Theory]
    [InlineData("../private")]
    [InlineData("..\\private")]
    [InlineData("/sessions/private")]
    [InlineData("C:\\private")]
    [InlineData("nested/private")]
    [InlineData("nested\\private")]
    public async Task Summary_rejects_malicious_session_selector(string sessionId)
    {
        var workspace = new InMemoryMspWorkspace();

        var result = await new WorkflowCommand().ExecuteAsync(
            CreateContext(workspace),
            new[] { "summary", sessionId });

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.Equal("msp.workflow.invalid_session_id", Assert.Single(result.Diagnostics).Code);
    }

    [Fact]
    public async Task Named_workflow_rejects_malicious_current_session_id()
    {
        var workspace = new InMemoryMspWorkspace();
        var invocation = new MspCommandInvocation
        {
            SessionId = "../documents/private",
            CommandText = "workflow run summarize-current",
            CommandName = "workflow"
        };

        var result = await new WorkflowCommand().ExecuteAsync(
            CreateContext(workspace, invocation),
            new[] { "run", "summarize-current" });

        Assert.False(result.Succeeded);
        Assert.Equal("msp.workflow.invalid_session_id", Assert.Single(result.Diagnostics).Code);
    }

    [Fact]
    public async Task Summary_rejects_invalid_id_inside_session_record()
    {
        var workspace = new InMemoryMspWorkspace();
        await WriteJsonAsync(workspace, "/sessions/safe.json", new MspSessionRecord
        {
            Id = "../other"
        });

        var result = await new WorkflowCommand().ExecuteAsync(
            CreateContext(workspace),
            new[] { "summary", "safe" });

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.Equal("msp.workflow.invalid_session_id", Assert.Single(result.Diagnostics).Code);
    }

    [Theory]
    [InlineData("../documents/secret")]
    [InlineData("..\\documents\\secret")]
    [InlineData("/documents/secret")]
    [InlineData("C:\\documents\\secret")]
    public async Task Summary_does_not_follow_malicious_transcript_id(string transcriptId)
    {
        var workspace = new InMemoryMspWorkspace();
        await WriteJsonAsync(workspace, "/sessions/safe.json", new MspSessionRecord
        {
            Id = "safe",
            TranscriptIds = new[] { transcriptId }
        });
        var unsafeTranscriptPath = workspace.NormalizePath($"/transcripts/{transcriptId}.json");
        await WriteJsonAsync(workspace, unsafeTranscriptPath, new MspCommandTranscriptRecord
        {
            Id = "secret",
            SessionId = "safe",
            CommandText = "stolen-command"
        });

        var result = await new WorkflowCommand().ExecuteAsync(
            CreateContext(workspace),
            new[] { "summary", "safe" });

        Assert.True(result.Succeeded, result.Stderr);
        var diagnostic = Assert.Single(result.Diagnostics);
        Assert.Equal(MspDiagnosticSeverity.Warning, diagnostic.Severity);
        Assert.Equal("msp.workflow.invalid_transcript_id", diagnostic.Code);
        Assert.Equal(transcriptId, diagnostic.Target);
        Assert.DoesNotContain("stolen-command", result.Stdout, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Named_workflow_does_not_follow_source_artifact_path_outside_namespace()
    {
        var workspace = new InMemoryMspWorkspace();
        await WriteJsonAsync(workspace, "/sessions/safe.json", new MspSessionRecord
        {
            Id = "safe",
            ArtifactPaths = new[] { "../documents/secret" }
        });
        await WriteJsonAsync(workspace, "/documents/secret.manifest.json", new MspArtifact
        {
            Path = "/documents/secret",
            SourceDocuments = new[] { "private-document" }
        });
        var invocation = new MspCommandInvocation
        {
            SessionId = "safe",
            CommandText = "workflow run summarize-current --artifact /artifacts/result.md",
            CommandName = "workflow"
        };

        var result = await new WorkflowCommand().ExecuteAsync(
            CreateContext(workspace, invocation),
            new[] { "run", "summarize-current", "--artifact", "/artifacts/result.md" });

        Assert.True(result.Succeeded, result.Stderr);
        var artifact = Assert.Single(result.Artifacts);
        Assert.DoesNotContain("private-document", artifact.SourceDocuments);
        Assert.Contains(
            result.Diagnostics,
            diagnostic => diagnostic.Code == "msp.workflow.invalid_source_artifact_path");
    }

    private static MspCommandContext CreateContext(
        InMemoryMspWorkspace workspace,
        MspCommandInvocation? invocation = null)
    {
        return new MspCommandContext(
            workspace,
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink(),
            invocation: invocation);
    }

    private static ValueTask WriteJsonAsync<T>(
        InMemoryMspWorkspace workspace,
        string path,
        T value)
    {
        return workspace.WriteTextAsync(path, JsonSerializer.Serialize(value));
    }
}

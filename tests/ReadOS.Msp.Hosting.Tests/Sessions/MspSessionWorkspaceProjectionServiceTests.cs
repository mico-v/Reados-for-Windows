using ReadOS.Msp.Hosting.Sessions;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Tests.Sessions;

public sealed class MspSessionWorkspaceProjectionServiceTests
{
    [Fact]
    public void ListSessionEntries_projects_virtual_json_paths()
    {
        var service = new MspSessionWorkspaceProjectionService();
        var session = new MspSessionRecord
        {
            Id = "reados-workbench",
            Title = "ReadOS Workbench MSP Session",
            LastCommandText = "workspace info",
            LastProgressMessage = "complete",
            LastDiagnosticsSummary = "warning msp.test: check state",
            LastRecoveryHint = "Inspect transcript.",
            TranscriptIds = new[] { "transcript-1" },
            ArtifactPaths = new[] { "/artifacts/report.md" }
        };

        var entry = Assert.Single(service.ListSessionEntries(new[] { session }));

        Assert.Equal("/sessions/reados-workbench.json", entry.Path);
        Assert.Equal("reados-workbench.json", entry.Name);
        Assert.False(entry.IsDirectory);
        Assert.Equal(MspSessionWorkspaceProjectionService.JsonMediaType, entry.MediaType);
        Assert.True(entry.SizeBytes > 512);
    }

    [Fact]
    public void ListTranscriptEntries_projects_virtual_json_paths()
    {
        var service = new MspSessionWorkspaceProjectionService();
        var transcript = new MspCommandTranscriptRecord
        {
            Id = "transcript-1",
            CommandText = "workspace info",
            Stdout = "ok",
            ArtifactsSummary = "/artifacts/report.md",
            ProgressMessage = "complete",
            DiagnosticsSummary = "warning msp.test: check state",
            RecoveryHint = "Inspect transcript."
        };

        var entry = Assert.Single(service.ListTranscriptEntries(new[] { transcript }));

        Assert.Equal("/transcripts/transcript-1.json", entry.Path);
        Assert.Equal("transcript-1.json", entry.Name);
        Assert.False(entry.IsDirectory);
        Assert.Equal(MspSessionWorkspaceProjectionService.JsonMediaType, entry.MediaType);
        Assert.True(entry.SizeBytes > 256);
    }

    [Fact]
    public void FindSession_and_FindTranscript_resolve_case_insensitive_paths()
    {
        var service = new MspSessionWorkspaceProjectionService();
        var session = new MspSessionRecord { Id = "ReadOS-Workbench" };
        var transcript = new MspCommandTranscriptRecord
        {
            Id = "Transcript-1",
            CommandText = "workspace info"
        };

        var foundSession = service.FindSession(new[] { session }, "/sessions/reados-workbench.json");
        var foundTranscript = service.FindTranscript(new[] { transcript }, "/transcripts/transcript-1.json");

        Assert.Same(session, foundSession);
        Assert.Same(transcript, foundTranscript);
    }

    [Fact]
    public void FindSession_and_FindTranscript_ignore_non_record_paths()
    {
        var service = new MspSessionWorkspaceProjectionService();

        Assert.Null(service.FindSession(Array.Empty<MspSessionRecord>(), "/sessions"));
        Assert.Null(service.FindTranscript(Array.Empty<MspCommandTranscriptRecord>(), "/transcripts"));
        Assert.Null(service.FindSession(Array.Empty<MspSessionRecord>(), "/sessions/session.txt"));
        Assert.Null(service.FindTranscript(Array.Empty<MspCommandTranscriptRecord>(), "/artifacts/transcript.json"));
    }
}

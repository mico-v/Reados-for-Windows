using ReadOS.Msp.Hosting.Sessions;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Tests.Sessions;

public sealed class MspSessionProjectionServiceTests
{
    [Fact]
    public void ProjectSessions_normalizes_blank_transcript_session_and_projects_default_title()
    {
        var service = new MspSessionProjectionService();
        var now = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero);
        var transcript = CreateTranscript("entry-1", string.Empty, "workspace info", 1);

        var sessions = service.ProjectSessions(
            "default-session",
            new[] { transcript },
            Array.Empty<MspArtifact>(),
            now);

        var session = Assert.Single(sessions);
        Assert.Equal("default-session", session.Id);
        Assert.Equal(MspSessionProjectionService.DefaultWorkbenchSessionTitle, session.Title);
        Assert.Equal("workspace info", session.LastCommandText);
        Assert.Equal(new[] { "entry-1" }, session.TranscriptIds);
    }

    [Fact]
    public void ProjectSessions_projects_running_pending_failure_and_artifact_state()
    {
        var service = new MspSessionProjectionService();
        var transcripts = new[]
        {
            CreateTranscript("success", "session", "workspace info", 1),
            CreateTranscript(
                "pending",
                "session",
                "artifact write /artifacts/pending.md",
                2,
                decision: "RequireConfirmation",
                exitCode: 126),
            CreateTranscript(
                "running",
                "session",
                "pdf text current 1 3",
                3,
                decision: "Running",
                isRunning: true)
        };
        var artifacts = new[]
        {
            new MspArtifact { Path = "/artifacts/b.md", SessionId = "session" },
            new MspArtifact { Path = "/artifacts/a.md", SessionId = "session" }
        };

        var sessions = service.ProjectSessions("default-session", transcripts, artifacts);

        var session = Assert.Single(sessions);
        Assert.Equal("session", session.Id);
        Assert.Equal(3, session.CommandCount);
        Assert.Equal(1, session.RunningCount);
        Assert.Equal(1, session.PendingApprovalCount);
        Assert.Equal(1, session.ApprovalCount);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal("pdf text current 1 3", session.LastCommandText);
        Assert.Equal(new[] { "success", "pending", "running" }, session.TranscriptIds);
        Assert.Equal(new[] { "/artifacts/a.md", "/artifacts/b.md" }, session.ArtifactPaths);
    }

    [Fact]
    public void ProjectSessions_keeps_artifact_only_sessions()
    {
        var service = new MspSessionProjectionService();
        var now = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero);
        var artifact = new MspArtifact
        {
            Path = "/artifacts/report.md",
            SessionId = "artifact-session"
        };

        var sessions = service.ProjectSessions(
            "default-session",
            Array.Empty<MspCommandTranscriptRecord>(),
            new[] { artifact },
            now);

        var session = Assert.Single(sessions);
        Assert.Equal("artifact-session", session.Id);
        Assert.Equal(0, session.CommandCount);
        Assert.Equal(string.Empty, session.LastCommandText);
        Assert.Equal(now, session.StartedAt);
        Assert.Equal(now, session.UpdatedAt);
        Assert.Equal("/artifacts/report.md", Assert.Single(session.ArtifactPaths));
    }

    private static MspCommandTranscriptRecord CreateTranscript(
        string id,
        string sessionId,
        string commandText,
        int minute,
        string decision = "Allow",
        int exitCode = 0,
        bool isRunning = false)
    {
        var startedAt = new DateTimeOffset(2026, 7, 7, 10, minute, 0, TimeSpan.Zero);
        return new MspCommandTranscriptRecord
        {
            Id = id,
            SessionId = sessionId,
            Actor = "test-agent",
            CommandText = commandText,
            StartedAt = startedAt,
            CompletedAt = startedAt.AddSeconds(1),
            Decision = decision,
            ExitCode = exitCode,
            IsRunning = isRunning
        };
    }
}

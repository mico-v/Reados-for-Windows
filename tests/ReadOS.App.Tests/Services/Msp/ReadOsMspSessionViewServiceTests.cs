using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspSessionViewServiceTests
{
    [Fact]
    public void GetVisibleSessions_filters_by_id_title_last_command_and_diagnostics()
    {
        var service = new ReadOsMspSessionViewService();
        var sessions = new[]
        {
            CreateSession("id-match", "Alpha", "workspace info", string.Empty, 1),
            CreateSession("beta", "Title Match", "library list", string.Empty, 2),
            CreateSession("gamma", "Gamma", "pdf search current risk", string.Empty, 3),
            CreateSession("delta", "Delta", "workspace info", "error msp.target: missing", 4),
            CreateSession("other", "Other", "workspace info", string.Empty, 5)
        };

        var idMatch = service.GetVisibleSessions(sessions, "id-match");
        var titleMatch = service.GetVisibleSessions(sessions, "title");
        var commandMatch = service.GetVisibleSessions(sessions, "search current");
        var diagnosticMatch = service.GetVisibleSessions(sessions, "msp.target");

        Assert.Equal("id-match", Assert.Single(idMatch).Id);
        Assert.Equal("beta", Assert.Single(titleMatch).Id);
        Assert.Equal("gamma", Assert.Single(commandMatch).Id);
        Assert.Equal("delta", Assert.Single(diagnosticMatch).Id);
    }

    [Fact]
    public void GetVisibleSessions_sorts_newest_first_then_title()
    {
        var service = new ReadOsMspSessionViewService();
        var updatedAt = new DateTimeOffset(2026, 7, 7, 12, 0, 0, TimeSpan.Zero);
        var sessions = new[]
        {
            CreateSession("old", "Old", "workspace info", string.Empty, 1, updatedAt.AddMinutes(-1)),
            CreateSession("bravo", "bravo", "workspace info", string.Empty, 2, updatedAt),
            CreateSession("alpha", "Alpha", "workspace info", string.Empty, 3, updatedAt)
        };

        var visible = service.GetVisibleSessions(sessions, " ");

        Assert.Equal(new[] { "alpha", "bravo", "old" }, visible.Select(item => item.Id));
    }

    [Fact]
    public void ShouldClearSelection_detects_missing_selected_session_case_insensitively()
    {
        var service = new ReadOsMspSessionViewService();
        var selected = CreateSession("SESSION", "Session", "workspace info", string.Empty, 1);
        var visible = new[]
        {
            CreateSession("session", "Session", "workspace info", string.Empty, 2)
        };

        Assert.False(service.ShouldClearSelection(null, visible));
        Assert.False(service.ShouldClearSelection(selected, visible));
        Assert.True(service.ShouldClearSelection(selected, Array.Empty<MspSessionEntry>()));
    }

    [Fact]
    public void SelectSessionTranscript_routes_pending_or_failed_transcripts_to_policy()
    {
        var service = new ReadOsMspSessionViewService();
        var session = CreateSession("session", "Session", "artifact write", string.Empty, 1);
        session.TranscriptIds.Add("pending");
        var pending = new MspTranscriptEntry
        {
            Id = "pending",
            Decision = "RequireConfirmation",
            ExitCode = 126
        };

        var selection = service.SelectSessionTranscript(session, new[] { pending });

        Assert.Same(pending, selection.Transcript);
        Assert.Equal(InspectorTab.Policy, selection.InspectorTab);
    }

    [Fact]
    public void SelectSessionTranscript_routes_running_success_or_missing_transcript_to_run()
    {
        var service = new ReadOsMspSessionViewService();
        var session = CreateSession("session", "Session", "workspace info", string.Empty, 1);
        session.TranscriptIds.Add("running");
        var running = new MspTranscriptEntry
        {
            Id = "running",
            IsRunning = true,
            Decision = "Running",
            ExitCode = 0
        };

        var runningSelection = service.SelectSessionTranscript(session, new[] { running });
        var missingSelection = service.SelectSessionTranscript(session, Array.Empty<MspTranscriptEntry>());
        var nullSelection = service.SelectSessionTranscript(null, new[] { running });

        Assert.Same(running, runningSelection.Transcript);
        Assert.Equal(InspectorTab.Run, runningSelection.InspectorTab);
        Assert.Null(missingSelection.Transcript);
        Assert.Equal(InspectorTab.Run, missingSelection.InspectorTab);
        Assert.Null(nullSelection.Transcript);
        Assert.Equal(InspectorTab.Run, nullSelection.InspectorTab);
    }

    [Fact]
    public void SelectSessionTranscript_uses_first_matching_transcript_in_visible_order()
    {
        var service = new ReadOsMspSessionViewService();
        var session = CreateSession("session", "Session", "workspace info", string.Empty, 1);
        session.TranscriptIds.Add("older");
        session.TranscriptIds.Add("newer");
        var newer = new MspTranscriptEntry
        {
            Id = "newer",
            ExitCode = 0
        };
        var older = new MspTranscriptEntry
        {
            Id = "older",
            ExitCode = 0
        };

        var selection = service.SelectSessionTranscript(session, new[] { newer, older });

        Assert.Same(newer, selection.Transcript);
        Assert.Equal(InspectorTab.Run, selection.InspectorTab);
    }

    private static MspSessionEntry CreateSession(
        string id,
        string title,
        string lastCommandText,
        string diagnostics,
        int minute,
        DateTimeOffset? updatedAt = null)
    {
        return new MspSessionEntry
        {
            Id = id,
            Title = title,
            LastCommandText = lastCommandText,
            LastDiagnosticsSummary = diagnostics,
            UpdatedAt = updatedAt ?? new DateTimeOffset(2026, 7, 7, 12, minute, 0, TimeSpan.Zero)
        };
    }
}

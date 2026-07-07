using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsActiveMspCommandServiceTests
{
    [Fact]
    public void Begin_tracks_matching_running_entry_and_cancel_requests_token_cancellation()
    {
        var service = new ReadOsActiveMspCommandService();
        var entry = CreateRunningEntry("entry-1");
        using var registration = service.Begin(entry.Id, CancellationToken.None);

        var canceled = service.Cancel(entry);

        Assert.True(canceled);
        Assert.True(registration.IsCancellationRequested);
        Assert.True(service.CanCancel(entry));
    }

    [Fact]
    public void Cancel_rejects_null_non_running_or_different_entries()
    {
        var service = new ReadOsActiveMspCommandService();
        using var registration = service.Begin("entry-1", CancellationToken.None);

        var nullCanceled = service.Cancel(null);
        var stoppedCanceled = service.Cancel(new MspTranscriptEntry
        {
            Id = "entry-1",
            IsRunning = false
        });
        var differentCanceled = service.Cancel(CreateRunningEntry("entry-2"));

        Assert.False(nullCanceled);
        Assert.False(stoppedCanceled);
        Assert.False(differentCanceled);
        Assert.False(registration.IsCancellationRequested);
    }

    [Fact]
    public void Begin_links_parent_cancellation_token()
    {
        var service = new ReadOsActiveMspCommandService();
        using var parent = new CancellationTokenSource();
        using var registration = service.Begin("entry-1", parent.Token);

        parent.Cancel();

        Assert.True(registration.IsCancellationRequested);
    }

    [Fact]
    public void Disposing_current_registration_clears_active_command()
    {
        var service = new ReadOsActiveMspCommandService();
        var entry = CreateRunningEntry("entry-1");
        var registration = service.Begin(entry.Id, CancellationToken.None);

        Assert.True(service.CanCancel(entry));

        registration.Dispose();

        Assert.False(service.CanCancel(entry));
        Assert.False(service.Cancel(entry));
    }

    [Fact]
    public void Disposing_previous_registration_does_not_clear_newer_active_command()
    {
        var service = new ReadOsActiveMspCommandService();
        var oldEntry = CreateRunningEntry("old-entry");
        var newEntry = CreateRunningEntry("new-entry");
        var oldRegistration = service.Begin(oldEntry.Id, CancellationToken.None);
        using var newRegistration = service.Begin(newEntry.Id, CancellationToken.None);

        oldRegistration.Dispose();

        Assert.False(service.CanCancel(oldEntry));
        Assert.True(service.CanCancel(newEntry));
        Assert.True(service.Cancel(newEntry));
        Assert.True(newRegistration.IsCancellationRequested);
    }

    private static MspTranscriptEntry CreateRunningEntry(string id)
    {
        return new MspTranscriptEntry
        {
            Id = id,
            CommandText = "workspace info",
            IsRunning = true
        };
    }
}

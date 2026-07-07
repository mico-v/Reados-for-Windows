using ReadOS.Msp.Hosting.Commands;

namespace ReadOS.Msp.Hosting.Tests.Commands;

public sealed class MspActiveCommandRegistryTests
{
    [Fact]
    public void Begin_tracks_matching_running_command_and_cancel_requests_token_cancellation()
    {
        var registry = new MspActiveCommandRegistry();
        using var registration = registry.Begin("entry-1", CancellationToken.None);

        var canceled = registry.Cancel("entry-1", isRunning: true);

        Assert.True(canceled);
        Assert.True(registration.IsCancellationRequested);
        Assert.True(registry.CanCancel("entry-1", isRunning: true));
    }

    [Fact]
    public void Cancel_rejects_missing_stopped_or_different_commands()
    {
        var registry = new MspActiveCommandRegistry();
        using var registration = registry.Begin("entry-1", CancellationToken.None);

        var missingCanceled = registry.Cancel(null, isRunning: true);
        var stoppedCanceled = registry.Cancel("entry-1", isRunning: false);
        var differentCanceled = registry.Cancel("entry-2", isRunning: true);

        Assert.False(missingCanceled);
        Assert.False(stoppedCanceled);
        Assert.False(differentCanceled);
        Assert.False(registration.IsCancellationRequested);
    }

    [Fact]
    public void Begin_links_parent_cancellation_token()
    {
        var registry = new MspActiveCommandRegistry();
        using var parent = new CancellationTokenSource();
        using var registration = registry.Begin("entry-1", parent.Token);

        parent.Cancel();

        Assert.True(registration.IsCancellationRequested);
    }

    [Fact]
    public void Disposing_current_registration_clears_active_command()
    {
        var registry = new MspActiveCommandRegistry();
        var registration = registry.Begin("entry-1", CancellationToken.None);

        Assert.True(registry.CanCancel("entry-1", isRunning: true));

        registration.Dispose();

        Assert.False(registry.CanCancel("entry-1", isRunning: true));
        Assert.False(registry.Cancel("entry-1", isRunning: true));
    }

    [Fact]
    public void Disposing_previous_registration_does_not_clear_newer_active_command()
    {
        var registry = new MspActiveCommandRegistry();
        var oldRegistration = registry.Begin("old-entry", CancellationToken.None);
        using var newRegistration = registry.Begin("new-entry", CancellationToken.None);

        oldRegistration.Dispose();

        Assert.False(registry.CanCancel("old-entry", isRunning: true));
        Assert.True(registry.CanCancel("new-entry", isRunning: true));
        Assert.True(registry.Cancel("new-entry", isRunning: true));
        Assert.True(newRegistration.IsCancellationRequested);
    }
}

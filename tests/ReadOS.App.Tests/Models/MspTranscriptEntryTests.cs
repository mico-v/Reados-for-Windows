using ReadOS.App.Models;

namespace ReadOS.App.Tests.Models;

public sealed class MspTranscriptEntryTests
{
    [Fact]
    public void HasEffects_is_false_for_default_unset_and_None_values()
    {
        var entry = new MspTranscriptEntry();
        Assert.False(entry.HasEffects);

        entry.Effects = "None";
        Assert.False(entry.HasEffects);

        entry.Effects = "  NONE  ";
        Assert.False(entry.HasEffects);

        entry.Effects = string.Empty;
        Assert.False(entry.HasEffects);
    }

    [Fact]
    public void HasEffects_is_true_for_concrete_effect_text()
    {
        var entry = new MspTranscriptEntry { Effects = "WriteWorkspace" };
        Assert.True(entry.HasEffects);

        entry.Effects = "DeleteWorkspace";
        Assert.True(entry.HasEffects);
    }

    [Fact]
    public void TimingLabel_formats_start_and_completion_with_duration()
    {
        var entry = new MspTranscriptEntry
        {
            StartedAt = new DateTimeOffset(2026, 7, 30, 10, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 30, 10, 0, 5, TimeSpan.Zero)
        };

        var label = entry.TimingLabel;
        Assert.Contains("->", label);
        Assert.Contains("5.0s", label);
    }
}

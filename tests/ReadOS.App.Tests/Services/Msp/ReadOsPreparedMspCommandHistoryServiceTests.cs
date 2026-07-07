using System.Collections.ObjectModel;
using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsPreparedMspCommandHistoryServiceTests
{
    [Fact]
    public void Record_trims_command_and_inserts_newest_first_with_normalized_title()
    {
        var commands = new ObservableCollection<PreparedMspCommand>();
        var service = new ReadOsPreparedMspCommandHistoryService(8);
        var createdAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero);

        var first = service.Record(commands, "  workflow run explain-section  ", "已准备章节讲解工作流。", createdAt);
        var second = service.Record(commands, "workflow run extract-evidence", "Ready!", createdAt.AddMinutes(1));

        Assert.NotNull(first);
        Assert.Equal(2, commands.Count);
        Assert.Same(second, commands[0]);
        Assert.Same(first, commands[1]);
        Assert.Equal("workflow run explain-section", first.CommandText);
        Assert.Equal("workflow run explain-section", first.Detail);
        Assert.Equal("已准备章节讲解工作流", first.Title);
        Assert.Equal("Ready", second?.Title);
        Assert.Equal(createdAt, first.CreatedAt);
    }

    [Fact]
    public void Record_ignores_blank_commands()
    {
        var commands = new ObservableCollection<PreparedMspCommand>();
        var service = new ReadOsPreparedMspCommandHistoryService(8);

        var result = service.Record(commands, "   ", "ignored", DateTimeOffset.UtcNow);

        Assert.Null(result);
        Assert.Empty(commands);
    }

    [Fact]
    public void Record_promotes_duplicate_command_without_duplicating()
    {
        var commands = new ObservableCollection<PreparedMspCommand>();
        var service = new ReadOsPreparedMspCommandHistoryService(8);
        service.Record(commands, "workflow run explain-section", "Explain", DateTimeOffset.UtcNow);
        service.Record(commands, "workflow run extract-evidence", "Extract", DateTimeOffset.UtcNow.AddMinutes(1));

        var duplicate = service.Record(
            commands,
            "workflow run explain-section",
            "Explain again",
            DateTimeOffset.UtcNow.AddMinutes(2));

        Assert.Equal(2, commands.Count);
        Assert.Same(duplicate, commands[0]);
        Assert.Equal("workflow run explain-section", commands[0].CommandText);
        Assert.Equal("Explain again", commands[0].Title);
        Assert.Equal(1, commands.Count(command => command.CommandText == "workflow run explain-section"));
    }

    [Fact]
    public void Record_caps_history_to_configured_maximum()
    {
        var commands = new ObservableCollection<PreparedMspCommand>();
        var service = new ReadOsPreparedMspCommandHistoryService(3);

        for (var index = 1; index <= 5; index++)
        {
            service.Record(
                commands,
                $"workflow run command-{index}",
                $"Command {index}",
                DateTimeOffset.UtcNow.AddMinutes(index));
        }

        Assert.Equal(3, commands.Count);
        Assert.Equal(new[]
        {
            "workflow run command-5",
            "workflow run command-4",
            "workflow run command-3"
        }, commands.Select(command => command.CommandText));
    }

    [Fact]
    public void Promote_moves_existing_command_to_front_and_ignores_missing_or_first_command()
    {
        var commands = new ObservableCollection<PreparedMspCommand>();
        var service = new ReadOsPreparedMspCommandHistoryService(8);
        var first = service.Record(commands, "workflow run first", "First", DateTimeOffset.UtcNow);
        var second = service.Record(commands, "workflow run second", "Second", DateTimeOffset.UtcNow.AddMinutes(1));
        var third = service.Record(commands, "workflow run third", "Third", DateTimeOffset.UtcNow.AddMinutes(2));
        var missing = new PreparedMspCommand
        {
            CommandText = "workflow run missing"
        };

        service.Promote(commands, first!);

        Assert.Same(first, commands[0]);
        Assert.Equal(new[] { first, third, second }, commands);

        service.Promote(commands, first!);
        service.Promote(commands, missing);

        Assert.Equal(new[] { first, third, second }, commands);
    }

    [Fact]
    public void NormalizeTitle_uses_default_for_empty_title_and_trims_common_terminal_punctuation()
    {
        var service = new ReadOsPreparedMspCommandHistoryService(8);

        Assert.Equal("Prepared MSP command", service.NormalizeTitle("   "));
        Assert.Equal("已准备工作流", service.NormalizeTitle("已准备工作流。"));
        Assert.Equal("Ready", service.NormalizeTitle("Ready."));
        Assert.Equal("Ready", service.NormalizeTitle("Ready!"));
    }
}

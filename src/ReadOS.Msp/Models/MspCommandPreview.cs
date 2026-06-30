using System.Text;

namespace ReadOS.Msp.Models;

public sealed record MspCommandPreview
{
    public static MspCommandPreview Empty { get; } = new();

    public string Summary { get; init; } = string.Empty;

    public IReadOnlyList<string> Targets { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> Details { get; init; } = Array.Empty<string>();

    public bool IsEmpty =>
        string.IsNullOrWhiteSpace(Summary) &&
        Targets.Count == 0 &&
        Details.Count == 0;

    public static MspCommandPreview Create(
        string summary,
        IReadOnlyList<string>? targets = null,
        IReadOnlyList<string>? details = null)
    {
        return new MspCommandPreview
        {
            Summary = summary,
            Targets = targets ?? Array.Empty<string>(),
            Details = details ?? Array.Empty<string>()
        };
    }

    public string ToDisplayText()
    {
        if (IsEmpty)
        {
            return string.Empty;
        }

        var builder = new StringBuilder();
        if (!string.IsNullOrWhiteSpace(Summary))
        {
            builder.AppendLine(Summary.Trim());
        }

        foreach (var target in Targets.Where(target => !string.IsNullOrWhiteSpace(target)))
        {
            builder.Append("target: ");
            builder.AppendLine(target.Trim());
        }

        foreach (var detail in Details.Where(detail => !string.IsNullOrWhiteSpace(detail)))
        {
            builder.Append("- ");
            builder.AppendLine(detail.Trim());
        }

        return builder.ToString().Trim();
    }
}

using System.Text;
using System.Text.RegularExpressions;
using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

public sealed class ReadOsMspAgentBridgeService
{
    public string BuildInstruction()
    {
        var builder = new StringBuilder();
        builder.AppendLine("你可以请求 ReadOS 执行 MSP 命令。MSP 命令面向你这个 Agent，不是让用户手动执行。");
        builder.AppendLine("当你需要读取工作区、资料、PDF 文本、搜索结果或安全 Windows 宿主信息时，返回一个 msp 代码块，每行一个命令：");
        builder.AppendLine("```msp");
        builder.AppendLine("workspace info");
        builder.AppendLine("library list");
        builder.AppendLine("pdf inspect current");
        builder.AppendLine("pdf search current \"关键词\"");
        builder.AppendLine("pdf search current \"关键词\" --artifact /artifacts/search.tsv");
        builder.AppendLine("pdf text current 1 3");
        builder.AppendLine("pdf text current 1 3 --artifact /artifacts/excerpt.txt");
        builder.AppendLine("artifact write /artifacts/summary.md \"摘要内容\"");
        builder.AppendLine("artifact list /artifacts");
        builder.AppendLine("artifact show /artifacts/summary.md");
        builder.AppendLine("workflow summary current");
        builder.AppendLine("workflow summary current --artifact /artifacts/workflows/current.md");
        builder.AppendLine("page-label set current 12 \"iii\"");
        builder.AppendLine("outline add current 42 \"Chapter 3\" --level 1");
        builder.AppendLine("attach page current 12");
        builder.AppendLine("attach range current 12 18");
        builder.AppendLine("chat ask current \"解释已附加页面\"");
        builder.AppendLine("chat ask current \"解释已附加页面\" --artifact /artifacts/chat/answer.md");
        builder.AppendLine("windows info");
        builder.AppendLine("windows path current");
        builder.AppendLine("```");
        builder.AppendLine("ReadOS 会解析这些命令，通过 MSP runtime 翻译到受控的 ReadOS 服务和 Windows/.NET API，然后把 stdout、stderr、exitCode、policy audit、diagnostics 和 recovery 返回给你。");
        builder.AppendLine("不要调用 PowerShell、cmd、bash 或主机文件系统路径；只使用 MSP 虚拟路径和已列出的命令。");
        builder.AppendLine("如果已有足够上下文，直接回答；如果需要执行命令，先只输出 msp 代码块和极短说明。");
        return builder.ToString().Trim();
    }

    public IReadOnlyList<string> ExtractRequestedCommands(string content)
    {
        var commands = new List<string>();
        foreach (Match match in Regex.Matches(
            content,
            @"```(?:reados[-_])?msp(?:-sh)?\s*(.*?)```",
            RegexOptions.IgnoreCase | RegexOptions.Singleline))
        {
            AddCommandsFromBlock(commands, match.Groups[1].Value);
        }

        foreach (Match match in Regex.Matches(
            content,
            @"<msp>\s*(.*?)</msp>",
            RegexOptions.IgnoreCase | RegexOptions.Singleline))
        {
            AddCommandsFromBlock(commands, match.Groups[1].Value);
        }

        foreach (Match match in Regex.Matches(
            content,
            @"<reados-msp>\s*(.*?)</reados-msp>",
            RegexOptions.IgnoreCase | RegexOptions.Singleline))
        {
            AddCommandsFromBlock(commands, match.Groups[1].Value);
        }

        return commands;
    }

    public string BuildExecutionReport(IReadOnlyList<MspTranscriptEntry> entries)
    {
        var builder = new StringBuilder();
        builder.AppendLine("MSP command results:");
        foreach (var entry in entries)
        {
            builder.AppendLine();
            builder.AppendLine($"$ {entry.CommandText}");
            builder.AppendLine($"exitCode: {entry.ExitCode}");
            builder.AppendLine($"decision: {entry.Decision}");
            builder.AppendLine($"effects: {entry.Effects}");
            if (!string.IsNullOrWhiteSpace(entry.ArtifactsSummary))
            {
                builder.AppendLine($"artifacts: {entry.ArtifactsSummary}");
            }

            if (!string.IsNullOrWhiteSpace(entry.Stdout))
            {
                builder.AppendLine("stdout:");
                builder.AppendLine(TrimForReport(entry.Stdout, 5000));
            }

            if (!string.IsNullOrWhiteSpace(entry.Stderr))
            {
                builder.AppendLine("stderr:");
                builder.AppendLine(TrimForReport(entry.Stderr, 2000));
            }

            if (!string.IsNullOrWhiteSpace(entry.DiagnosticsSummary))
            {
                builder.AppendLine("diagnostics:");
                builder.AppendLine(TrimForReport(entry.DiagnosticsSummary, 2000));
            }

            if (!string.IsNullOrWhiteSpace(entry.RecoveryHint))
            {
                builder.AppendLine($"recovery: {entry.RecoveryHint}");
            }
        }

        return builder.ToString().Trim();
    }

    private static void AddCommandsFromBlock(List<string> commands, string block)
    {
        foreach (var rawLine in block.Replace("\r\n", "\n").Split('\n'))
        {
            var command = rawLine.Trim();
            if (string.IsNullOrWhiteSpace(command) ||
                command.StartsWith('#') ||
                command.StartsWith("//", StringComparison.Ordinal))
            {
                continue;
            }

            if (command.StartsWith("$ ", StringComparison.Ordinal))
            {
                command = command[2..].Trim();
            }

            if (command.StartsWith("msp>", StringComparison.OrdinalIgnoreCase))
            {
                command = command[4..].Trim();
            }

            if (!string.IsNullOrWhiteSpace(command))
            {
                commands.Add(command);
            }
        }
    }

    private static string TrimForReport(string value, int maxLength)
    {
        var trimmed = value.Trim();
        return trimmed.Length <= maxLength ? trimmed : trimmed[..maxLength] + "...";
    }
}

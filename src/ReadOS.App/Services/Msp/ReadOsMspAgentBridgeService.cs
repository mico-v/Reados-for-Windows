using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Runtime;

namespace ReadOS.App.Services.Msp;

public sealed record ReadOsMspAgentDispatch
{
    public required string Operation { get; init; }

    public MspExecSessionRequest? ExecRequest { get; init; }

    public MspExecSessionStdinRequest? StdinRequest { get; init; }

    public MspExecSessionRequest? Exec => ExecRequest;

    public MspExecSessionStdinRequest? Stdin => StdinRequest;

    public bool IsExec => ExecRequest is not null;

    public bool IsStdin => StdinRequest is not null;
}

public sealed class ReadOsMspAgentBridgeService
{
    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private static readonly JsonDocumentOptions JsonOptions = new()
    {
        AllowTrailingCommas = false,
        CommentHandling = JsonCommentHandling.Disallow,
        MaxDepth = 32
    };

    private static readonly string[] ExecArgumentKeys =
    [
        "mode",
        "cmd",
        "command",
        "commandText",
        "program",
        "arguments",
        "workspaceRoot",
        "environment",
        "workdir",
        "workingDirectory",
        "yield_time_ms",
        "yieldTimeMs",
        "max_output_tokens",
        "maxOutputTokens"
    ];

    private static readonly string[] StdinArgumentKeys =
    [
        "session_id",
        "sessionId",
        "chars",
        "yield_time_ms",
        "yieldTimeMs",
        "max_output_tokens",
        "maxOutputTokens"
    ];

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

    /// <summary>
    /// Parses the model-facing JSON shape into a typed request for the Hosting
    /// exec-session facade. No execution or provider/UI work occurs here.
    /// </summary>
    public ReadOsMspAgentDispatch ParseJsonDispatch(
        string operation,
        string jsonArguments)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);
        ArgumentNullException.ThrowIfNull(jsonArguments);

        try
        {
            using var document = JsonDocument.Parse(jsonArguments, JsonOptions);
            return ParseJsonDispatch(operation, document.RootElement);
        }
        catch (JsonException)
        {
            throw InvalidJson($"{operation} arguments must be a valid JSON object.");
        }
    }

    /// <summary>Parses a JSON element into a typed Hosting-facade dispatch.</summary>
    public ReadOsMspAgentDispatch ParseJsonDispatch(
        string operation,
        JsonElement jsonArguments)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);

        return operation switch
        {
            "exec_command" => new ReadOsMspAgentDispatch
            {
                Operation = operation,
                ExecRequest = ParseExec(jsonArguments)
            },
            "write_stdin" => new ReadOsMspAgentDispatch
            {
                Operation = operation,
                StdinRequest = ParseStdin(jsonArguments)
            },
            _ => throw new ArgumentException(
                "The JSON exec-session operation is not supported.",
                nameof(operation))
        };
    }

    /// <summary>Compatibility name for the typed JSON dispatch entry point.</summary>
    public ReadOsMspAgentDispatch ParseJson(
        string operation,
        string jsonArguments)
    {
        return ParseJsonDispatch(operation, jsonArguments);
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

    private static MspExecSessionRequest ParseExec(JsonElement jsonArguments)
    {
        var properties = ReadObject(jsonArguments, "exec", ExecArgumentKeys);
        var mode = ParseMode(properties);
        var command = ParseCommand(properties, mode);
        var program = GetOptionalString(properties, "program", "exec");
        if (program?.IndexOf('\0') >= 0)
        {
            throw Invalid("exec program must not contain NUL characters.");
        }

        var arguments = GetArguments(properties, mode);
        var workspaceRoot = GetOptionalString(properties, "workspaceRoot", "exec");
        if (workspaceRoot?.IndexOf('\0') >= 0)
        {
            throw Invalid("exec workspaceRoot must not contain NUL characters.");
        }

        var environment = GetEnvironment(properties, mode);
        var workingDirectory = GetOptionalAliasedString(
            properties,
            "exec",
            "workdir",
            "workingDirectory");
        var yieldTimeMs = GetAliasedNonNegativeInt32(
            properties,
            "exec",
            "yield_time_ms",
            "yieldTimeMs");
        var maxOutputTokens = GetAliasedNonNegativeInt32(
            properties,
            "exec",
            "max_output_tokens",
            "maxOutputTokens");

        if (mode == MspExecSessionMode.Shell)
        {
            if (program is not null || arguments is not null ||
                workspaceRoot is not null || environment is not null)
            {
                throw Invalid(
                    "exec process fields require mode 'process'.");
            }

            return new MspExecSessionRequest
            {
                Mode = mode,
                CommandText = command,
                WorkingDirectory = workingDirectory,
                YieldTimeMs = yieldTimeMs,
                MaxOutputTokens = maxOutputTokens
            };
        }

        if (command is not null)
        {
            throw Invalid(
                "exec command fields are not valid for mode 'process'.");
        }

        if (string.IsNullOrWhiteSpace(program))
        {
            throw Invalid("exec process mode requires program.");
        }

        if (string.IsNullOrWhiteSpace(workspaceRoot))
        {
            throw Invalid("exec process mode requires workspaceRoot.");
        }

        return new MspExecSessionRequest
        {
            Mode = mode,
            Program = program,
            Arguments = arguments,
            WorkspaceRoot = workspaceRoot,
            Environment = environment,
            WorkingDirectory = workingDirectory,
            YieldTimeMs = yieldTimeMs,
            MaxOutputTokens = maxOutputTokens
        };
    }

    private static MspExecSessionStdinRequest ParseStdin(JsonElement jsonArguments)
    {
        var properties = ReadObject(jsonArguments, "write_stdin", StdinArgumentKeys);
        var sessionId = GetAliasedSessionId(properties);
        var chars = GetOptionalNullableString(properties, "chars", "write_stdin");
        if (chars is not null &&
            chars.Length > MspNativeExecSessionLimits.MaximumWriteStdinChars)
        {
            throw Invalid(
                $"write_stdin chars exceed the maximum of {MspNativeExecSessionLimits.MaximumWriteStdinChars} characters.");
        }

        return new MspExecSessionStdinRequest
        {
            SessionId = sessionId,
            Chars = chars,
            YieldTimeMs = GetAliasedNonNegativeInt32(
                properties,
                "write_stdin",
                "yield_time_ms",
                "yieldTimeMs"),
            MaxOutputTokens = GetAliasedNonNegativeInt32(
                properties,
                "write_stdin",
                "max_output_tokens",
                "maxOutputTokens")
        };
    }

    private static Dictionary<string, JsonElement> ReadObject(
        JsonElement value,
        string operation,
        IReadOnlyList<string> allowedKeys)
    {
        if (value.ValueKind != JsonValueKind.Object)
        {
            throw Invalid($"{operation} arguments must be a JSON object.");
        }

        var allowed = new HashSet<string>(allowedKeys, StringComparer.Ordinal);
        var properties = new Dictionary<string, JsonElement>(StringComparer.Ordinal);
        foreach (var property in value.EnumerateObject())
        {
            if (!allowed.Contains(property.Name) ||
                !properties.TryAdd(property.Name, property.Value))
            {
                throw Invalid(
                    $"{operation} arguments contain unsupported or duplicate keys.");
            }
        }

        return properties;
    }

    private static MspExecSessionMode ParseMode(
        IReadOnlyDictionary<string, JsonElement> properties)
    {
        if (!properties.TryGetValue("mode", out var value))
        {
            return MspExecSessionMode.Shell;
        }

        if (value.ValueKind != JsonValueKind.String)
        {
            throw Invalid("exec mode must be 'shell' or 'process'.");
        }

        return value.GetString() switch
        {
            "shell" => MspExecSessionMode.Shell,
            "process" => MspExecSessionMode.Process,
            _ => throw Invalid("exec mode must be 'shell' or 'process'.")
        };
    }

    private static string? ParseCommand(
        IReadOnlyDictionary<string, JsonElement> properties,
        MspExecSessionMode mode)
    {
        if (!HasAny(properties, ["cmd", "commandText", "command"]))
        {
            if (mode == MspExecSessionMode.Shell)
            {
                throw Invalid("exec shell mode requires cmd.");
            }

            return null;
        }

        var command = GetAliasedString(
            properties,
            "exec",
            "cmd",
            "commandText",
            "command");
        if (mode == MspExecSessionMode.Shell && string.IsNullOrWhiteSpace(command))
        {
            throw Invalid("exec shell mode requires cmd.");
        }

        if (command?.IndexOf('\0') >= 0)
        {
            throw Invalid("exec command must not contain NUL characters.");
        }

        return command;
    }

    private static IReadOnlyList<string>? GetArguments(
        IReadOnlyDictionary<string, JsonElement> properties,
        MspExecSessionMode mode)
    {
        if (!properties.TryGetValue("arguments", out var value))
        {
            return null;
        }

        if (mode != MspExecSessionMode.Process)
        {
            throw Invalid("exec arguments require mode 'process'.");
        }

        if (value.ValueKind != JsonValueKind.Array)
        {
            throw Invalid("exec arguments must be an array of strings.");
        }

        if (value.GetArrayLength() > MspNativeExecSessionLimits.MaximumProcessArguments)
        {
            throw Invalid(
                $"exec arguments exceed the maximum of {MspNativeExecSessionLimits.MaximumProcessArguments} entries.");
        }

        var arguments = new List<string>(value.GetArrayLength());
        foreach (var argument in value.EnumerateArray())
        {
            if (argument.ValueKind != JsonValueKind.String)
            {
                throw Invalid("exec arguments must be an array of strings.");
            }

            var text = argument.GetString();
            if (text is null || text.IndexOf('\0') >= 0)
            {
                throw Invalid(
                    "exec arguments must be printable strings without NUL characters.");
            }

            if (text.Length > MspNativeExecSessionLimits.MaximumArgumentCharacters)
            {
                throw Invalid(
                    $"exec arguments must not exceed {MspNativeExecSessionLimits.MaximumArgumentCharacters} characters per entry.");
            }

            arguments.Add(text);
        }

        return arguments;
    }

    private static IReadOnlyDictionary<string, string>? GetEnvironment(
        IReadOnlyDictionary<string, JsonElement> properties,
        MspExecSessionMode mode)
    {
        if (!properties.TryGetValue("environment", out var value))
        {
            return null;
        }

        if (mode != MspExecSessionMode.Process)
        {
            throw Invalid("exec environment requires mode 'process'.");
        }

        if (value.ValueKind != JsonValueKind.Object)
        {
            throw Invalid("exec environment must be an object of strings.");
        }

        var environment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var entryCount = 0;
        foreach (var property in value.EnumerateObject())
        {
            if (++entryCount > MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries)
            {
                throw Invalid(
                    $"exec environment exceeds the maximum of {MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries} entries.");
            }

            var name = property.Name;
            if (name.Length == 0 || name.Contains('=') || ContainsControl(name) ||
                !environment.TryAdd(name, string.Empty))
            {
                throw Invalid(
                    "exec environment names must be unique printable strings without '='.");
            }

            if (property.Value.ValueKind != JsonValueKind.String)
            {
                throw Invalid("exec environment must be an object of strings.");
            }

            var text = property.Value.GetString();
            if (text is null || ContainsControl(text))
            {
                throw Invalid("exec environment values must be printable strings.");
            }

            var normalizedName = name.ToLowerInvariant();
            if (normalizedName is "path" or "pwd" or "systemroot")
            {
                throw Invalid("exec environment cannot override mandatory names.");
            }

            int entryBytes;
            try
            {
                entryBytes = checked(
                    StrictUtf8.GetByteCount(name) + StrictUtf8.GetByteCount(text));
            }
            catch (EncoderFallbackException)
            {
                throw Invalid("exec environment values must be valid UTF-8 text.");
            }

            if (entryBytes > MspNativeExecSessionLimits.MaximumProcessEnvironmentEntryBytes)
            {
                throw Invalid(
                    $"exec environment entries must not exceed {MspNativeExecSessionLimits.MaximumProcessEnvironmentEntryBytes} UTF-8 bytes combined.");
            }

            environment[name] = text;
        }

        return environment;
    }

    private static ulong GetAliasedSessionId(
        IReadOnlyDictionary<string, JsonElement> properties)
    {
        var value = GetAliased(
            properties,
            "write_stdin",
            "session_id",
            "sessionId");
        if (value.ValueKind != JsonValueKind.Number ||
            !value.TryGetUInt64(out var sessionId) || sessionId == 0)
        {
            throw Invalid("write_stdin session_id must be a non-zero integer.");
        }

        return sessionId;
    }

    private static string? GetOptionalString(
        IReadOnlyDictionary<string, JsonElement> properties,
        string name,
        string operation)
    {
        if (!properties.TryGetValue(name, out var value))
        {
            return null;
        }

        if (value.ValueKind != JsonValueKind.String)
        {
            throw Invalid($"{operation} {name} must be a string.");
        }

        var text = value.GetString();
        if (text is null)
        {
            throw Invalid($"{operation} {name} must be a string.");
        }

        return text;
    }

    private static string? GetOptionalNullableString(
        IReadOnlyDictionary<string, JsonElement> properties,
        string name,
        string operation)
    {
        if (!properties.TryGetValue(name, out var value))
        {
            return null;
        }

        if (value.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        return GetOptionalString(properties, name, operation);
    }

    private static string? GetOptionalAliasedString(
        IReadOnlyDictionary<string, JsonElement> properties,
        string operation,
        params string[] names)
    {
        if (!HasAny(properties, names))
        {
            return null;
        }

        return GetAliasedString(properties, operation, names);
    }

    private static string? GetAliasedString(
        IReadOnlyDictionary<string, JsonElement> properties,
        string operation,
        params string[] names)
    {
        var value = GetAliased(properties, operation, names);
        if (value.ValueKind != JsonValueKind.String)
        {
            throw Invalid($"{operation} {names[0]} must be a string.");
        }

        var text = value.GetString();
        if (text is null)
        {
            throw Invalid($"{operation} {names[0]} must be a string.");
        }

        return text;
    }

    private static int? GetAliasedNonNegativeInt32(
        IReadOnlyDictionary<string, JsonElement> properties,
        string operation,
        params string[] names)
    {
        if (!HasAny(properties, names))
        {
            return null;
        }

        var value = GetAliased(properties, operation, names);
        if (value.ValueKind != JsonValueKind.Number ||
            !value.TryGetInt32(out var number) || number < 0)
        {
            throw Invalid($"{operation} {names[0]} must be a non-negative integer.");
        }

        return number;
    }

    private static JsonElement GetAliased(
        IReadOnlyDictionary<string, JsonElement> properties,
        string operation,
        params string[] names)
    {
        JsonElement? result = null;
        foreach (var name in names)
        {
            if (!properties.TryGetValue(name, out var value))
            {
                continue;
            }

            if (result.HasValue)
            {
                throw Invalid($"{operation} arguments contain duplicate aliases.");
            }

            result = value;
        }

        if (!result.HasValue)
        {
            throw Invalid($"{operation} arguments are missing a required field.");
        }

        return result.Value;
    }

    private static bool HasAny(
        IReadOnlyDictionary<string, JsonElement> properties,
        IReadOnlyList<string> names)
    {
        foreach (var name in names)
        {
            if (properties.ContainsKey(name))
            {
                return true;
            }
        }

        return false;
    }

    private static bool ContainsControl(string value)
    {
        return value.Any(char.IsControl);
    }

    private static ArgumentException Invalid(string message)
    {
        return new ArgumentException(message, "jsonArguments");
    }

    private static ArgumentException InvalidJson(string message)
    {
        return new ArgumentException(message, "jsonArguments");
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

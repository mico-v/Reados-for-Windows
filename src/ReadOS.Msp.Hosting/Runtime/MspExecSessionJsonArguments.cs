using System.Text;
using System.Text.Json;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Runtime;

/// <summary>
/// Parses the model-facing JSON argument objects for the typed exec-session
/// facade. The parser deliberately maps JSON into the existing managed request
/// records; it never deserializes native request or response envelopes.
/// </summary>
internal static class MspExecSessionJsonArguments
{
    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private static readonly JsonDocumentOptions DocumentOptions = new()
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

    internal static MspExecSessionRequest ParseExec(string jsonArguments)
    {
        ArgumentNullException.ThrowIfNull(jsonArguments);

        try
        {
            using var document = JsonDocument.Parse(jsonArguments, DocumentOptions);
            return ParseExec(document.RootElement);
        }
        catch (JsonException)
        {
            throw Invalid("exec arguments must be a valid JSON object.");
        }
    }

    internal static MspExecSessionRequest ParseExec(JsonElement jsonArguments)
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

    internal static MspExecSessionStdinRequest ParseStdin(string jsonArguments)
    {
        ArgumentNullException.ThrowIfNull(jsonArguments);

        try
        {
            using var document = JsonDocument.Parse(jsonArguments, DocumentOptions);
            return ParseStdin(document.RootElement);
        }
        catch (JsonException)
        {
            throw Invalid("write_stdin arguments must be a valid JSON object.");
        }
    }

    internal static MspExecSessionStdinRequest ParseStdin(JsonElement jsonArguments)
    {
        var properties = ReadObject(jsonArguments, "write_stdin", StdinArgumentKeys);
        var sessionId = GetAliasedSessionId(properties);
        var chars = GetOptionalNullableString(properties, "chars", "write_stdin");
        if (chars is not null && chars.Length > MspNativeExecSessionLimits.MaximumWriteStdinChars)
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
                throw Invalid($"{operation} arguments contain unsupported or duplicate keys.");
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
                throw Invalid("exec arguments must be printable strings without NUL characters.");
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

        if (value.EnumerateObject().Count() >
            MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries)
        {
            throw Invalid(
                $"exec environment exceeds the maximum of {MspNativeExecSessionLimits.MaximumProcessEnvironmentEntries} entries.");
        }

        var environment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var property in value.EnumerateObject())
        {
            var name = property.Name;
            if (name.Length == 0 || name.Contains('=') || ContainsControl(name) ||
                !environment.TryAdd(name, string.Empty))
            {
                throw Invalid("exec environment names must be unique printable strings without '='.");
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
                entryBytes = checked(StrictUtf8.GetByteCount(name) + StrictUtf8.GetByteCount(text));
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
}

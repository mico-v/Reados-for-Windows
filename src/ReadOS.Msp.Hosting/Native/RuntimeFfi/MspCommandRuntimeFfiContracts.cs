using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ReadOS.Msp.Hosting.Native.RuntimeFfi;

public enum MspCommandRuntimeFfiStatus
{
    Ok = 0,
    InvalidArgument = 1,
    LimitExceeded = 2,
    InternalError = 3,
    Panic = 4
}

public enum MspCommandRuntimeFfiFailureKind
{
    LibraryUnavailable,
    ExportUnavailable,
    AbiMismatch,
    VersionMismatch,
    InvalidArgument,
    LimitExceeded,
    NativeInternalError,
    NativePanic,
    NullHandle,
    NullResult,
    InvalidBuffer,
    InvalidDiagnostic,
    CrossLibraryHandle,
    Disposed,
    UnknownNativeStatus
}

public sealed record MspCommandRuntimeFfiLimits
{
    public const int DefaultMaximumJsonRequestBytes = 4 * 1024 * 1024;
    public const int DefaultMaximumCommandBytes = 128 * 1024;
    public const int DefaultMaximumVirtualPathBytes = 4 * 1024;
    public const int DefaultMaximumFileBytes = 8 * 1024 * 1024;
    public const int DefaultMaximumWorkspaceBytes = 64 * 1024 * 1024;
    public const int DefaultMaximumWorkspaceFiles = 65_536;
    public const int DefaultMaximumDirectoryEntries = 65_536;
    public const int DefaultMaximumStdinBytes = 2 * 1024 * 1024;
    public const int DefaultMaximumOutputBytes = 2 * 1024 * 1024;
    public const int DefaultMaximumDiagnosticBytes = 4 * 1024;
    public const int DefaultMaximumVariables = 256;
    public const int DefaultMaximumVariableNameBytes = 64;
    public const int DefaultMaximumVariableValueBytes = 64 * 1024;
    public const int DefaultMaximumVariableTotalBytes = 256 * 1024;

    public int MaximumJsonRequestBytes { get; init; } = DefaultMaximumJsonRequestBytes;
    public int MaximumCommandBytes { get; init; } = DefaultMaximumCommandBytes;
    public int MaximumVirtualPathBytes { get; init; } = DefaultMaximumVirtualPathBytes;
    public int MaximumFileBytes { get; init; } = DefaultMaximumFileBytes;
    public int MaximumWorkspaceBytes { get; init; } = DefaultMaximumWorkspaceBytes;
    public int MaximumWorkspaceFiles { get; init; } = DefaultMaximumWorkspaceFiles;
    public int MaximumDirectoryEntries { get; init; } = DefaultMaximumDirectoryEntries;
    public int MaximumStdinBytes { get; init; } = DefaultMaximumStdinBytes;
    public int MaximumOutputBytes { get; init; } = DefaultMaximumOutputBytes;
    public int MaximumDiagnosticBytes { get; init; } = DefaultMaximumDiagnosticBytes;
    public int MaximumVariables { get; init; } = DefaultMaximumVariables;
    public int MaximumVariableNameBytes { get; init; } = DefaultMaximumVariableNameBytes;
    public int MaximumVariableValueBytes { get; init; } = DefaultMaximumVariableValueBytes;
    public int MaximumVariableTotalBytes { get; init; } = DefaultMaximumVariableTotalBytes;

    internal void Validate()
    {
        ValidateBound(MaximumJsonRequestBytes, DefaultMaximumJsonRequestBytes, nameof(MaximumJsonRequestBytes));
        ValidateBound(MaximumCommandBytes, DefaultMaximumCommandBytes, nameof(MaximumCommandBytes));
        ValidateBound(MaximumVirtualPathBytes, DefaultMaximumVirtualPathBytes, nameof(MaximumVirtualPathBytes));
        ValidateBound(MaximumFileBytes, DefaultMaximumFileBytes, nameof(MaximumFileBytes));
        ValidateBound(MaximumWorkspaceBytes, DefaultMaximumWorkspaceBytes, nameof(MaximumWorkspaceBytes));
        ValidateBound(MaximumWorkspaceFiles, DefaultMaximumWorkspaceFiles, nameof(MaximumWorkspaceFiles));
        ValidateBound(MaximumDirectoryEntries, DefaultMaximumDirectoryEntries, nameof(MaximumDirectoryEntries));
        ValidateBound(MaximumStdinBytes, DefaultMaximumStdinBytes, nameof(MaximumStdinBytes));
        ValidateBound(MaximumOutputBytes, DefaultMaximumOutputBytes, nameof(MaximumOutputBytes));
        ValidateBound(MaximumDiagnosticBytes, DefaultMaximumDiagnosticBytes, nameof(MaximumDiagnosticBytes));
        ValidateBound(MaximumVariables, DefaultMaximumVariables, nameof(MaximumVariables));
        ValidateBound(MaximumVariableNameBytes, DefaultMaximumVariableNameBytes, nameof(MaximumVariableNameBytes));
        ValidateBound(MaximumVariableValueBytes, DefaultMaximumVariableValueBytes, nameof(MaximumVariableValueBytes));
        ValidateBound(MaximumVariableTotalBytes, DefaultMaximumVariableTotalBytes, nameof(MaximumVariableTotalBytes));
    }

    private static void ValidateBound(int value, int maximum, string parameterName)
    {
        if (value <= 0 || value > maximum)
        {
            throw new ArgumentOutOfRangeException(parameterName, "The configured FFI limit is outside the supported range.");
        }
    }
}

public sealed record MspCommandRuntimeFfiOptions
{
    public MspCommandRuntimeFfiLimits Limits { get; init; } = new();

    internal string ValidateAndGetFullPath(string fullyQualifiedLibraryPath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(fullyQualifiedLibraryPath);
        ArgumentNullException.ThrowIfNull(Limits);
        Limits.Validate();

        if (!Path.IsPathFullyQualified(fullyQualifiedLibraryPath))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LibraryUnavailable);
        }

        try
        {
            var fullPath = Path.GetFullPath(fullyQualifiedLibraryPath);
            if (!string.Equals(Path.GetExtension(fullPath), ".dll", StringComparison.OrdinalIgnoreCase))
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LibraryUnavailable);
            }

            return fullPath;
        }
        catch (Exception exception) when (exception is ArgumentException or NotSupportedException or PathTooLongException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LibraryUnavailable);
        }
    }
}

public sealed class MspCommandRuntimeFfiException : Exception
{
    private MspCommandRuntimeFfiException(
        MspCommandRuntimeFfiFailureKind failureKind,
        MspCommandRuntimeFfiStatus? status = null)
        : base(MessageFor(failureKind))
    {
        FailureKind = failureKind;
        Status = status;
    }

    public MspCommandRuntimeFfiFailureKind FailureKind { get; }

    public MspCommandRuntimeFfiStatus? Status { get; }

    internal static MspCommandRuntimeFfiException Create(
        MspCommandRuntimeFfiFailureKind failureKind,
        MspCommandRuntimeFfiStatus? status = null)
    {
        return new MspCommandRuntimeFfiException(failureKind, status);
    }

    internal static MspCommandRuntimeFfiException ForStatus(int status)
    {
        if (!Enum.IsDefined(typeof(MspCommandRuntimeFfiStatus), status))
        {
            return Create(MspCommandRuntimeFfiFailureKind.UnknownNativeStatus);
        }

        var value = (MspCommandRuntimeFfiStatus)status;
        return value switch
        {
            MspCommandRuntimeFfiStatus.Ok => throw new ArgumentException("A successful status is not an exception status.", nameof(status)),
            MspCommandRuntimeFfiStatus.InvalidArgument => Create(MspCommandRuntimeFfiFailureKind.InvalidArgument, value),
            MspCommandRuntimeFfiStatus.LimitExceeded => Create(MspCommandRuntimeFfiFailureKind.LimitExceeded, value),
            MspCommandRuntimeFfiStatus.InternalError => Create(MspCommandRuntimeFfiFailureKind.NativeInternalError, value),
            MspCommandRuntimeFfiStatus.Panic => Create(MspCommandRuntimeFfiFailureKind.NativePanic, value),
            _ => Create(MspCommandRuntimeFfiFailureKind.UnknownNativeStatus)
        };
    }

    private static string MessageFor(MspCommandRuntimeFfiFailureKind failureKind)
    {
        return failureKind switch
        {
            MspCommandRuntimeFfiFailureKind.LibraryUnavailable => "The command runtime FFI library is unavailable.",
            MspCommandRuntimeFfiFailureKind.ExportUnavailable => "The command runtime FFI library does not expose the required contract.",
            MspCommandRuntimeFfiFailureKind.AbiMismatch => "The command runtime FFI ABI is incompatible.",
            MspCommandRuntimeFfiFailureKind.VersionMismatch => "The command runtime FFI header version is incompatible.",
            MspCommandRuntimeFfiFailureKind.InvalidArgument => "The command runtime FFI rejected the argument.",
            MspCommandRuntimeFfiFailureKind.LimitExceeded => "The command runtime FFI limit was exceeded.",
            MspCommandRuntimeFfiFailureKind.NativeInternalError => "The command runtime FFI reported an internal error.",
            MspCommandRuntimeFfiFailureKind.NativePanic => "The command runtime FFI reported a panic.",
            MspCommandRuntimeFfiFailureKind.NullHandle => "The command runtime FFI returned a null handle.",
            MspCommandRuntimeFfiFailureKind.NullResult => "The command runtime FFI returned a null result.",
            MspCommandRuntimeFfiFailureKind.InvalidBuffer => "The command runtime FFI returned an invalid buffer.",
            MspCommandRuntimeFfiFailureKind.InvalidDiagnostic => "The command runtime FFI returned an invalid diagnostic.",
            MspCommandRuntimeFfiFailureKind.CrossLibraryHandle => "The command runtime FFI handles belong to different libraries.",
            MspCommandRuntimeFfiFailureKind.Disposed => "The command runtime FFI object has been disposed.",
            MspCommandRuntimeFfiFailureKind.UnknownNativeStatus => "The command runtime FFI returned an unknown status.",
            _ => "The command runtime FFI operation failed."
        };
    }
}

public sealed record MspCommandRuntimeFfiRequest
{
    public uint SchemaVersion { get; init; } = MspCommandRuntimeFfiAbi.RequestSchemaVersion;

    public required string Command { get; init; }

    public required string Cwd { get; init; }

    public byte[]? StdinBytes { get; init; }

    public IReadOnlyDictionary<string, string>? Variables { get; init; }

    public bool ErrorOnUnbound { get; init; }
}

public sealed class MspCommandRuntimeFfiResult : IDisposable
{
    private readonly IDisposable nativeOwner;
    private int disposed;

    internal MspCommandRuntimeFfiResult(
        int exitCode,
        byte[] stdoutBytes,
        byte[] stderrBytes,
        byte[] diagnosticUtf8,
        string diagnosticCode,
        IDisposable nativeOwner)
    {
        ExitCode = exitCode;
        StdoutBytes = stdoutBytes;
        StderrBytes = stderrBytes;
        DiagnosticUtf8 = diagnosticUtf8;
        DiagnosticCode = diagnosticCode;
        this.nativeOwner = nativeOwner;
    }

    public int ExitCode { get; }

    public byte[] StdoutBytes { get; }

    public byte[] StderrBytes { get; }

    public byte[] DiagnosticUtf8 { get; }

    public string DiagnosticCode { get; }

    public ReadOnlyMemory<byte> Stdout => StdoutBytes;

    public ReadOnlyMemory<byte> Stderr => StderrBytes;

    public ReadOnlyMemory<byte> Diagnostic => DiagnosticUtf8;

    public void Dispose()
    {
        if (Interlocked.Exchange(ref disposed, 1) == 0)
        {
            nativeOwner.Dispose();
        }

        GC.SuppressFinalize(this);
    }
}

public interface IMspCommandRuntimeFfiAdapter : IDisposable
{
    uint AbiVersion { get; }

    string HeaderVersion { get; }

    MspCommandRuntimeFfiRuntime CreateRuntime();

    MspCommandRuntimeFfiWorkspace CreateWorkspace();

    MspCommandRuntimeFfiResult ExecuteJson(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace,
        ReadOnlySpan<byte> requestJsonUtf8);

    MspCommandRuntimeFfiResult Execute(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace,
        MspCommandRuntimeFfiRequest request);
}

internal static class MspCommandRuntimeFfiUtf8
{
    internal static readonly Encoding Strict = new UTF8Encoding(false, true);

    internal static byte[] Encode(string value, int maximum, bool rejectControl = true)
    {
        ArgumentNullException.ThrowIfNull(value);
        byte[] bytes;
        try
        {
            bytes = Strict.GetBytes(value);
        }
        catch (EncoderFallbackException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        if (bytes.Length > maximum)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        if (bytes.Contains((byte)0) || rejectControl && value.Any(char.IsControl))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        return bytes;
    }

    internal static bool IsValidVirtualPath(string value, bool allowRoot, int maximumBytes)
    {
        if (string.IsNullOrEmpty(value) || !value.StartsWith("/", StringComparison.Ordinal) ||
            value.Contains('\\') || value.Contains(':') || value.Contains('\0') ||
            value.Any(char.IsControl) || value.Contains("//", StringComparison.Ordinal) ||
            value != "/" && value.EndsWith("/", StringComparison.Ordinal))
        {
            return false;
        }

        byte[] bytes;
        try
        {
            bytes = Strict.GetBytes(value);
        }
        catch (EncoderFallbackException)
        {
            return false;
        }

        if (bytes.Length > maximumBytes || value == "/" && !allowRoot)
        {
            return false;
        }

        foreach (var component in value.Split('/').Skip(1))
        {
            if (component is "." or ".." || string.Equals(component, ".msp", StringComparison.OrdinalIgnoreCase))
            {
                return false;
            }
        }

        return true;
    }
}

internal sealed class MspCommandRuntimeFfiRequestWire
{
    [JsonPropertyName("version")]
    public uint Version { get; init; }

    public required string Command { get; init; }

    public required string Cwd { get; init; }

    [JsonPropertyName("stdinBase64")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? StdinBase64 { get; init; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyDictionary<string, string>? Variables { get; init; }

    [JsonPropertyName("errorOnUnbound")]
    public bool ErrorOnUnbound { get; init; }
}

internal static class MspCommandRuntimeFfiRequestSerializer
{
    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DictionaryKeyPolicy = null,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
        WriteIndented = false
    };

    private static readonly HashSet<string> AllowedWireProperties =
    [
        "version",
        "command",
        "cwd",
        "stdinBase64",
        "variables",
        "errorOnUnbound"
    ];

    internal static byte[] Serialize(
        MspCommandRuntimeFfiRequest request,
        MspCommandRuntimeFfiLimits limits)
    {
        ArgumentNullException.ThrowIfNull(request);
        ArgumentNullException.ThrowIfNull(limits);
        if (request.SchemaVersion != MspCommandRuntimeFfiAbi.RequestSchemaVersion || request.Command is null || request.Cwd is null)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        _ = MspCommandRuntimeFfiUtf8.Encode(request.Cwd, limits.MaximumVirtualPathBytes);
        if (!MspCommandRuntimeFfiUtf8.IsValidVirtualPath(
                request.Cwd,
                allowRoot: false,
                limits.MaximumVirtualPathBytes))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        _ = MspCommandRuntimeFfiUtf8.Encode(request.Command, limits.MaximumCommandBytes);
        _ = MspCommandRuntimeFfiUtf8.Encode(request.Cwd, limits.MaximumVirtualPathBytes);
        if (request.StdinBytes is not null && request.StdinBytes.Length > limits.MaximumStdinBytes)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        string? stdinBase64 = request.StdinBytes is null
            ? null
            : Convert.ToBase64String(request.StdinBytes);
        var variables = ValidateVariables(request.Variables, limits);
        var wire = new MspCommandRuntimeFfiRequestWire
        {
            Version = request.SchemaVersion,
            Command = request.Command,
            Cwd = request.Cwd,
            StdinBase64 = stdinBase64,
            Variables = variables,
            ErrorOnUnbound = request.ErrorOnUnbound
        };

        byte[] json;
        try
        {
            json = JsonSerializer.SerializeToUtf8Bytes(wire, Options);
        }
        catch (Exception exception) when (exception is JsonException or NotSupportedException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        if (json.Length == 0 || json.Length > limits.MaximumJsonRequestBytes || json.Contains((byte)0))
        {
            throw MspCommandRuntimeFfiException.Create(
                json.Length > limits.MaximumJsonRequestBytes
                    ? MspCommandRuntimeFfiFailureKind.LimitExceeded
                    : MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        return json;
    }

    internal static void ValidateRaw(
        ReadOnlyMemory<byte> requestJsonUtf8,
        MspCommandRuntimeFfiLimits limits)
    {
        ArgumentNullException.ThrowIfNull(limits);
        if (requestJsonUtf8.Length == 0)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        if (requestJsonUtf8.Length > limits.MaximumJsonRequestBytes)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        if (requestJsonUtf8.Span.Contains((byte)0))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        var requestSpan = requestJsonUtf8.Span;
        if (requestSpan.Length >= 3 && requestSpan[0] == 0xEF &&
            requestSpan[1] == 0xBB && requestSpan[2] == 0xBF)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        try
        {
            using var document = JsonDocument.Parse(requestJsonUtf8);
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
            }

            var seen = new HashSet<string>(StringComparer.Ordinal);
            var hasVersion = false;
            var hasCommand = false;
            var hasCwd = false;
            foreach (var property in root.EnumerateObject())
            {
                if (!AllowedWireProperties.Contains(property.Name) || !seen.Add(property.Name))
                {
                    throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
                }

                switch (property.Name)
                {
                    case "version":
                        hasVersion = true;
                        if (!property.Value.TryGetUInt32(out var version) || version != MspCommandRuntimeFfiAbi.RequestSchemaVersion)
                        {
                            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
                        }

                        break;
                    case "command":
                        hasCommand = true;
                        ValidateRawString(property.Value, limits.MaximumCommandBytes);
                        break;
                    case "cwd":
                        hasCwd = true;
                        if (property.Value.ValueKind != JsonValueKind.String)
                        {
                            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
                        }

                        var cwd = property.Value.GetString();
                        ValidateRawVirtualPath(cwd, limits);
                        break;
                    case "stdinBase64":
                        if (property.Value.ValueKind != JsonValueKind.Null)
                        {
                            if (property.Value.ValueKind != JsonValueKind.String)
                            {
                                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
                            }

                            ValidateRawBase64(property.Value.GetString(), limits);
                        }

                        break;
                    case "variables":
                        ValidateRawVariables(property.Value, limits);
                        break;
                    case "errorOnUnbound":
                        if (property.Value.ValueKind is not JsonValueKind.True and not JsonValueKind.False)
                        {
                            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
                        }

                        break;
                }
            }

            if (!hasVersion || !hasCommand || !hasCwd)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
            }
        }
        catch (MspCommandRuntimeFfiException)
        {
            throw;
        }
        catch (JsonException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }
    }

    private static void ValidateRawVirtualPath(
        string? value,
        MspCommandRuntimeFfiLimits limits)
    {
        if (value is null)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        _ = MspCommandRuntimeFfiUtf8.Encode(value, limits.MaximumVirtualPathBytes);
        if (!MspCommandRuntimeFfiUtf8.IsValidVirtualPath(
                value,
                allowRoot: false,
                limits.MaximumVirtualPathBytes))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }
    }

    private static void ValidateRawString(JsonElement value, int maximum)
    {
        if (value.ValueKind != JsonValueKind.String || value.GetString() is not { } text)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        _ = MspCommandRuntimeFfiUtf8.Encode(text, maximum);
    }

    private static void ValidateRawBase64(string? value, MspCommandRuntimeFfiLimits limits)
    {
        if (value is null || value.Length > limits.MaximumJsonRequestBytes ||
            !value.All(char.IsAscii) || value.Length % 4 != 0)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        var maximumQuartetOutput = checked(value.Length / 4 * 3);
        if (maximumQuartetOutput > limits.MaximumStdinBytes + 2)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        byte[] decoded;
        try
        {
            decoded = Convert.FromBase64String(value);
        }
        catch (FormatException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        if (decoded.Length > limits.MaximumStdinBytes)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        if (!string.Equals(Convert.ToBase64String(decoded), value, StringComparison.Ordinal))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }
    }

    private static void ValidateRawVariables(JsonElement value, MspCommandRuntimeFfiLimits limits)
    {
        if (value.ValueKind != JsonValueKind.Object)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        var properties = value.EnumerateObject();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var count = 0;
        var total = 0;
        foreach (var property in properties)
        {
            if (++count > limits.MaximumVariables)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
            }

            if (!seen.Add(property.Name) ||
                property.Value.ValueKind != JsonValueKind.String ||
                property.Value.GetString() is not { } variableValue ||
                property.Name.Length == 0 ||
                !property.Name.All(character => character == '_' || char.IsAsciiLetterOrDigit(character)) ||
                !char.IsAsciiLetter(property.Name[0]))
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
            }

            var nameBytes = MspCommandRuntimeFfiUtf8.Encode(property.Name, limits.MaximumVariableNameBytes);
            var valueBytes = MspCommandRuntimeFfiUtf8.Encode(variableValue, limits.MaximumVariableValueBytes);
            total = checked(total + nameBytes.Length + valueBytes.Length);
            if (total > limits.MaximumVariableTotalBytes)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
            }
        }
    }

    private static IReadOnlyDictionary<string, string>? ValidateVariables(
        IReadOnlyDictionary<string, string>? variables,
        MspCommandRuntimeFfiLimits limits)
    {
        if (variables is null)
        {
            return null;
        }

        if (variables.Count > limits.MaximumVariables)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        var result = new SortedDictionary<string, string>(StringComparer.Ordinal);
        var total = 0;
        foreach (var pair in variables)
        {
            if (pair.Key is null || pair.Value is null ||
                pair.Key.Length == 0 ||
                !pair.Key.All(character => character == '_' || char.IsAsciiLetterOrDigit(character)) ||
                !char.IsAsciiLetter(pair.Key[0]))
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
            }

            var name = MspCommandRuntimeFfiUtf8.Encode(pair.Key, limits.MaximumVariableNameBytes);
            var value = MspCommandRuntimeFfiUtf8.Encode(pair.Value, limits.MaximumVariableValueBytes);
            total = checked(total + name.Length + value.Length);
            if (total > limits.MaximumVariableTotalBytes)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
            }

            if (!result.TryAdd(pair.Key, pair.Value))
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
            }
        }

        return result;
    }
}

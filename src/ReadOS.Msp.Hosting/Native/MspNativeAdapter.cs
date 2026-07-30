using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ReadOS.Msp.Hosting.Native;

public interface IMspNativeAdapter : IDisposable
{
    MspNativeCommandResult Execute(MspNativeCommandRequest request);

    MspNativeShellParseResult Parse(MspNativeShellParseRequest request);

    MspNativeWorkspacePathResult NormalizeWorkspacePath(MspNativeWorkspacePathRequest request);
}

public sealed class MspNativeAdapter : IMspNativeAdapter, IMspNativeRuntimeInfoProvider
{
    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private static readonly JsonSerializerOptions JsonOptions = CreateJsonOptions();

    private readonly IMspNativeTransport transport;
    private readonly MspNativeAdapterLimits limits;
    private bool disposed;

    public MspNativeAdapter(
        IMspNativeTransport transport,
        MspNativeAdapterLimits? limits = null)
    {
        this.transport = transport ?? throw new ArgumentNullException(nameof(transport));
        this.limits = limits ?? new MspNativeAdapterLimits();
        this.limits.Validate();
    }

    public static MspNativeAdapter LoadWindows(MspNativeLibraryOptions options)
    {
        ArgumentNullException.ThrowIfNull(options);
        return new MspNativeAdapter(
            new WindowsMspNativeTransport(options),
            options.Limits);
    }

    public static MspNativeAdapter LoadPackagedWindows(
        MspNativeAdapterLimits? limits = null)
    {
        var resolvedLimits = limits ?? new MspNativeAdapterLimits();
        return LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = Path.Combine(AppContext.BaseDirectory, "msp_core.dll"),
            Limits = resolvedLimits
        });
    }

    public MspNativeRuntimeInfo NativeRuntimeInfo =>
        transport is IMspNativeRuntimeInfoProvider infoProvider
            ? infoProvider.NativeRuntimeInfo
            : MspNativeRuntimeInfo.Unknown;

    public MspNativeCommandResult Execute(MspNativeCommandRequest request)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        ArgumentNullException.ThrowIfNull(request);
        ValidateCommandRequest(request);

        var wireRequest = new MspNativeCommandRequestWire
        {
            CommandText = request.CommandText,
            WorkingDirectory = request.WorkingDirectory,
            Actor = request.Actor,
            SessionId = request.SessionId,
            DryRun = request.DryRun,
            Environment = request.Environment,
            WorkspaceRoot = request.WorkspaceRoot
        };

        var wireResult = InvokeAndDeserialize<MspNativeCommandResultWire>(
            MspNativeOperation.Execute,
            wireRequest);
        ValidateContractVersion(wireResult.ContractVersion, MspNativeOperation.Execute);

        var stdoutBytes = DecodeAuthoritativeBytes(
            wireResult.StdoutBytesBase64,
            MspNativeOperation.Execute);
        byte[] stderrBytes;
        try
        {
            stderrBytes = DecodeAuthoritativeBytes(
                wireResult.StderrBytesBase64,
                MspNativeOperation.Execute);
        }
        catch
        {
            CryptographicOperations.ZeroMemory(stdoutBytes);
            throw;
        }

        try
        {
            if (wireResult.ExitCode is null)
            {
                throw InvalidResponse(MspNativeOperation.Execute);
            }

            ValidateCompatibilityProjection(
                wireResult.Stdout,
                stdoutBytes,
                MspNativeOperation.Execute);
            ValidateCompatibilityProjection(
                wireResult.Stderr,
                stderrBytes,
                MspNativeOperation.Execute);
            var auditRecords = ValidateAuditRecords(
                wireResult.AuditRecords,
                request,
                wireResult.ExitCode.Value,
                wireResult.Diagnostics,
                MspNativeOperation.Execute);
            var diagnostics = ValidateDiagnostics(
                wireResult.Diagnostics,
                MspNativeOperation.Execute);
            ValidateStateChange(wireResult.StateChange, MspNativeOperation.Execute);

            var result = new MspNativeCommandResult(
                wireResult.ExitCode.Value,
                stdoutBytes,
                stderrBytes,
                wireResult.StateChange,
                auditRecords,
                diagnostics);
            EnsureWorkspaceRootIsNotDisclosed(result, request.WorkspaceRoot);
            CryptographicOperations.ZeroMemory(stdoutBytes);
            CryptographicOperations.ZeroMemory(stderrBytes);
            return result;
        }
        catch
        {
            CryptographicOperations.ZeroMemory(stdoutBytes);
            CryptographicOperations.ZeroMemory(stderrBytes);
            throw;
        }
    }

    public MspNativeShellParseResult Parse(MspNativeShellParseRequest request)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        ArgumentNullException.ThrowIfNull(request);
        ArgumentNullException.ThrowIfNull(request.CommandText);

        var result = InvokeAndDeserialize<MspNativeShellParseResult>(
            MspNativeOperation.Parse,
            new MspNativeShellParseRequestWire
            {
                CommandText = request.CommandText
            });
        ValidateContractVersion(result.ContractVersion, MspNativeOperation.Parse);
        ValidateParseResult(result, request.CommandText);
        return FreezeParseResult(result);
    }

    public MspNativeWorkspacePathResult NormalizeWorkspacePath(
        MspNativeWorkspacePathRequest request)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        ArgumentNullException.ThrowIfNull(request);
        ArgumentNullException.ThrowIfNull(request.Path);
        ArgumentNullException.ThrowIfNull(request.CurrentDirectory);

        var result = InvokeAndDeserialize<MspNativeWorkspacePathResult>(
            MspNativeOperation.NormalizeWorkspacePath,
            new MspNativeWorkspacePathRequestWire
            {
                Path = request.Path,
                CurrentDirectory = request.CurrentDirectory
            });
        ValidateContractVersion(
            result.ContractVersion,
            MspNativeOperation.NormalizeWorkspacePath);
        ValidateWorkspacePathResult(result);
        return result;
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        transport.Dispose();
    }

    private TResponse InvokeAndDeserialize<TResponse>(
        MspNativeOperation operation,
        object request)
    {
        byte[] requestJson;
        try
        {
            requestJson = JsonSerializer.SerializeToUtf8Bytes(request, JsonOptions);
        }
        catch (Exception exception) when (exception is JsonException or NotSupportedException)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvocationFailed,
                operation);
        }

        try
        {
            if (requestJson.Length == 0 || requestJson.Length > limits.MaximumRequestBytes)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    operation);
            }

            byte[] responseJson;
            try
            {
                responseJson = transport.Invoke(operation, requestJson);
            }
            catch (MspNativeAdapterException)
            {
                throw;
            }
            catch (Exception exception) when (!IsFatal(exception))
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    operation);
            }

            if (responseJson is null)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.NullResponse,
                    operation);
            }

            try
            {
                if (responseJson.Length == 0)
                {
                    throw MspNativeAdapterException.Create(
                        MspNativeFailureKind.InvalidJson,
                        operation);
                }

                if (responseJson.Length > limits.MaximumResponseBytes)
                {
                    throw MspNativeAdapterException.Create(
                        MspNativeFailureKind.ResponseTooLarge,
                        operation);
                }

                try
                {
                    _ = StrictUtf8.GetCharCount(responseJson);
                }
                catch (DecoderFallbackException)
                {
                    throw MspNativeAdapterException.Create(
                        MspNativeFailureKind.InvalidUtf8,
                        operation);
                }

                try
                {
                    return JsonSerializer.Deserialize<TResponse>(responseJson, JsonOptions)
                        ?? throw MspNativeAdapterException.Create(
                            MspNativeFailureKind.InvalidJson,
                            operation);
                }
                catch (JsonException)
                {
                    throw MspNativeAdapterException.Create(
                        MspNativeFailureKind.InvalidJson,
                        operation);
                }
            }
            finally
            {
                CryptographicOperations.ZeroMemory(responseJson);
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(requestJson);
        }
    }

    private byte[] DecodeAuthoritativeBytes(
        string? encoded,
        MspNativeOperation operation)
    {
        if (encoded is null || !IsStrictBase64(encoded, out var decodedLength))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvalidBase64,
                operation);
        }

        if (decodedLength > limits.MaximumDecodedStreamBytes)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.ResponseTooLarge,
                operation);
        }

        if (decodedLength == 0)
        {
            return Array.Empty<byte>();
        }

        var decoded = new byte[decodedLength];
        if (!Convert.TryFromBase64String(encoded, decoded, out var bytesWritten) ||
            bytesWritten != decodedLength)
        {
            CryptographicOperations.ZeroMemory(decoded);
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvalidBase64,
                operation);
        }

        return decoded;
    }

    private static bool IsStrictBase64(string value, out int decodedLength)
    {
        decodedLength = 0;
        if (value.Length == 0)
        {
            return true;
        }

        if (value.Length % 4 != 0)
        {
            return false;
        }

        var padding = value.EndsWith("==", StringComparison.Ordinal)
            ? 2
            : value.EndsWith('=')
                ? 1
                : 0;
        for (var index = 0; index < value.Length; index++)
        {
            var character = value[index];
            var isPaddingPosition = index >= value.Length - padding;
            if (character == '=')
            {
                if (!isPaddingPosition)
                {
                    return false;
                }

                continue;
            }

            if (isPaddingPosition ||
                !(character is >= 'A' and <= 'Z' or
                    >= 'a' and <= 'z' or
                    >= '0' and <= '9' or '+' or '/'))
            {
                return false;
            }
        }

        try
        {
            decodedLength = checked((value.Length / 4 * 3) - padding);
            return true;
        }
        catch (OverflowException)
        {
            return false;
        }
    }

    private static void ValidateCommandRequest(MspNativeCommandRequest request)
    {
        ArgumentNullException.ThrowIfNull(request.CommandText);
        ArgumentNullException.ThrowIfNull(request.WorkingDirectory);
        ArgumentNullException.ThrowIfNull(request.Actor);
        ArgumentNullException.ThrowIfNull(request.SessionId);
        ArgumentNullException.ThrowIfNull(request.Environment);
        ValidateEvidenceIdentity(request.Actor, nameof(request.Actor));
        ValidateEvidenceIdentity(request.SessionId, nameof(request.SessionId));
        if (!IsNormalizedVirtualPath(request.WorkingDirectory))
        {
            throw new ArgumentException(
                "WorkingDirectory must be a normalized model-visible absolute path.",
                nameof(request));
        }

        if (request.WorkspaceRoot is not null &&
            string.IsNullOrWhiteSpace(request.WorkspaceRoot))
        {
            throw new ArgumentException(
                "WorkspaceRoot must be null or a non-empty host path.",
                nameof(request));
        }

        if (request.WorkspaceRoot is { } workspaceRoot &&
            !IsFullyQualifiedWindowsHostPath(workspaceRoot))
        {
            throw new ArgumentException(
                "WorkspaceRoot must be a fully qualified Windows host path.",
                nameof(request));
        }
    }

    private static void ValidateEvidenceIdentity(string value, string parameterName)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Length > 256 || value.Any(char.IsControl))
        {
            throw new ArgumentException(
                "Native MSP evidence identities must be non-empty, bounded, printable text.",
                parameterName);
        }
    }

    private static void ValidateContractVersion(
        string? contractVersion,
        MspNativeOperation operation)
    {
        if (string.IsNullOrWhiteSpace(contractVersion))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvalidResponse,
                operation);
        }

        if (!string.Equals(
                contractVersion,
                MspNativeContract.Version,
                StringComparison.Ordinal))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.UnsupportedContractVersion,
                operation);
        }
    }

    private static IReadOnlyList<MspNativeAuditRecord> ValidateAuditRecords(
        IReadOnlyList<MspNativeAuditRecord>? records,
        MspNativeCommandRequest request,
        int resultExitCode,
        IReadOnlyList<MspNativeDiagnostic>? resultDiagnostics,
        MspNativeOperation operation)
    {
        if (records is null || records.Count != 1 || resultDiagnostics is null)
        {
            throw InvalidResponse(operation);
        }

        foreach (var record in records)
        {
            if (record is null ||
                string.IsNullOrWhiteSpace(record.RunId) ||
                record.CommandLine is null ||
                record.Arguments is null ||
                record.Actor is null ||
                record.SessionId is null ||
                record.WorkingDirectory is null ||
                record.PolicyDecision is null ||
                record.Diagnostics is null ||
                record.EndedAtUnixMs < record.StartedAtUnixMs ||
                record.ExitCode != resultExitCode ||
                !string.Equals(record.CommandLine, request.CommandText, StringComparison.Ordinal) ||
                !string.Equals(record.Actor, request.Actor, StringComparison.Ordinal) ||
                !string.Equals(record.SessionId, request.SessionId, StringComparison.Ordinal) ||
                !string.Equals(
                    record.WorkingDirectory,
                    request.WorkingDirectory,
                    StringComparison.Ordinal) ||
                !record.Diagnostics.SequenceEqual(resultDiagnostics))
            {
                throw InvalidResponse(operation);
            }

            foreach (var argument in record.Arguments)
            {
                if (argument is null)
                {
                    throw InvalidResponse(operation);
                }
            }

            ValidateDiagnostics(record.Diagnostics, operation);
        }

        return records;
    }

    private static IReadOnlyList<MspNativeDiagnostic> ValidateDiagnostics(
        IReadOnlyList<MspNativeDiagnostic>? diagnostics,
        MspNativeOperation operation)
    {
        if (diagnostics is null)
        {
            throw InvalidResponse(operation);
        }

        foreach (var diagnostic in diagnostics)
        {
            if (diagnostic is null ||
                string.IsNullOrWhiteSpace(diagnostic.Code) ||
                diagnostic.Message is null)
            {
                throw InvalidResponse(operation);
            }
        }

        return diagnostics;
    }

    private static void ValidateStateChange(
        MspNativeCommandRuntimeStateChange? stateChange,
        MspNativeOperation operation)
    {
        if (stateChange?.CurrentDirectory is { } currentDirectory &&
            !IsNormalizedVirtualPath(currentDirectory))
        {
            throw InvalidResponse(operation);
        }
    }

    private static void ValidateParseResult(
        MspNativeShellParseResult result,
        string requestedCommandText)
    {
        const MspNativeOperation operation = MspNativeOperation.Parse;
        if (result.Succeeded)
        {
            if (result.Script is null || result.Error is not null ||
                !string.Equals(
                    result.Script.RawInput,
                    requestedCommandText,
                    StringComparison.Ordinal))
            {
                throw InvalidResponse(operation);
            }

            ValidateScript(result.Script, operation);
            return;
        }

        if (result.Script is not null || result.Error is null ||
            result.Error.Message is null || result.Error.ExitCode == 0)
        {
            throw InvalidResponse(operation);
        }
    }

    private static void ValidateScript(
        MspNativeParsedShellScript script,
        MspNativeOperation operation)
    {
        if (script.RawInput is null || script.Pipelines is null || script.Pipelines.Count == 0)
        {
            throw InvalidResponse(operation);
        }

        foreach (var pipeline in script.Pipelines)
        {
            if (pipeline is null || pipeline.Commands is null || pipeline.Commands.Count == 0 ||
                pipeline.PipeOperators is null ||
                pipeline.PipeOperators.Count != Math.Max(0, pipeline.Commands.Count - 1))
            {
                throw InvalidResponse(operation);
            }

            foreach (var command in pipeline.Commands)
            {
                ValidateParsedCommand(command, operation);
            }
        }
    }

    private static void ValidateParsedCommand(
        MspNativeParsedCommandLine? command,
        MspNativeOperation operation)
    {
        if (command is null || command.CommandName is null || command.Arguments is null ||
            command.Assignments is null || command.Redirections is null ||
            command.RawInput is null || command.ArgumentWords is null)
        {
            throw InvalidResponse(operation);
        }

        if (command.Arguments.Count != command.ArgumentWords.Count ||
            command.IsAssignmentOnly != (command.CommandNameWord is null))
        {
            throw InvalidResponse(operation);
        }

        foreach (var argument in command.Arguments)
        {
            if (argument is null)
            {
                throw InvalidResponse(operation);
            }
        }

        foreach (var assignment in command.Assignments)
        {
            if (assignment is null || assignment.Name is null || assignment.Value is null)
            {
                throw InvalidResponse(operation);
            }
        }

        ValidateWord(command.CommandNameWord, operation, allowNull: true);
        if (command.CommandNameWord is { } commandNameWord &&
            !string.Equals(
                command.CommandName,
                JoinWordParts(commandNameWord),
                StringComparison.Ordinal))
        {
            throw InvalidResponse(operation);
        }

        for (var index = 0; index < command.ArgumentWords.Count; index++)
        {
            var word = command.ArgumentWords[index];
            ValidateWord(word, operation, allowNull: false);
            if (!string.Equals(
                    command.Arguments[index],
                    JoinWordParts(word),
                    StringComparison.Ordinal))
            {
                throw InvalidResponse(operation);
            }
        }

        foreach (var redirection in command.Redirections)
        {
            if (redirection is null || redirection.Target is null)
            {
                throw InvalidResponse(operation);
            }

            ValidateWord(redirection.TargetWord, operation, allowNull: false);
            if (!string.Equals(
                    redirection.Target,
                    JoinWordParts(redirection.TargetWord),
                    StringComparison.Ordinal))
            {
                throw InvalidResponse(operation);
            }
        }
    }

    private static void ValidateWord(
        MspNativeParsedWord? word,
        MspNativeOperation operation,
        bool allowNull)
    {
        if (word is null)
        {
            if (allowNull)
            {
                return;
            }

            throw InvalidResponse(operation);
        }

        if (word.Parts is null)
        {
            throw InvalidResponse(operation);
        }

        foreach (var part in word.Parts)
        {
            if (part is null || part.Text is null)
            {
                throw InvalidResponse(operation);
            }
        }
    }

    private static string JoinWordParts(MspNativeParsedWord word)
    {
        return string.Concat(word.Parts.Select(part => part.Text));
    }

    private static void ValidateWorkspacePathResult(MspNativeWorkspacePathResult result)
    {
        const MspNativeOperation operation = MspNativeOperation.NormalizeWorkspacePath;
        if (result.Succeeded)
        {
            if (!IsNormalizedVirtualPath(result.VirtualPath) || result.Error is not null)
            {
                throw InvalidResponse(operation);
            }

            return;
        }

        if (result.VirtualPath is not null || string.IsNullOrWhiteSpace(result.Error))
        {
            throw InvalidResponse(operation);
        }
    }

    private static bool IsNormalizedVirtualPath(string? path)
    {
        if (string.IsNullOrEmpty(path) || !path.StartsWith('/') ||
            path.Contains('\\') || path.Contains(':') || path.Contains('\0'))
        {
            return false;
        }

        if (path != "/" && (path.EndsWith('/') || path.Contains("//", StringComparison.Ordinal)))
        {
            return false;
        }

        return path == "/" || path
            .Split('/', StringSplitOptions.RemoveEmptyEntries)
            .All(component => component is not "." and not "..");
    }

    private static void EnsureWorkspaceRootIsNotDisclosed(
        MspNativeCommandResult result,
        string? workspaceRoot)
    {
        if (string.IsNullOrWhiteSpace(workspaceRoot))
        {
            return;
        }

        var markers = GetHostPathMarkers(workspaceRoot);
        foreach (var bytes in new[] { result.StdoutBytes, result.StderrBytes })
        {
            if (ContainsEncodedHostPath(bytes.AsSpan(), markers))
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.HostPathDisclosure,
                    MspNativeOperation.Execute);
            }
        }

        foreach (var value in EnumerateResultStrings(result))
        {
            if (markers.Any(marker =>
                    value.Contains(marker, StringComparison.OrdinalIgnoreCase)))
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.HostPathDisclosure,
                    MspNativeOperation.Execute);
            }
        }
    }

    private static IReadOnlyList<string> GetHostPathMarkers(string workspaceRoot)
    {
        var markers = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        AddMarker(workspaceRoot);
        AddTrimmedMarker(workspaceRoot);
        try
        {
            var fullPath = Path.GetFullPath(workspaceRoot);
            AddMarker(fullPath);
            AddTrimmedMarker(fullPath);
        }
        catch (Exception exception) when (exception is ArgumentException or NotSupportedException or PathTooLongException)
        {
            // Rust owns final workspace-root authorization. Invalid roots still must not leak.
        }

        return markers.ToArray();

        void AddTrimmedMarker(string marker)
        {
            if (marker.Length <= 3 || marker[^1] is not ('/' or '\\') || marker[^2] == ':')
            {
                return;
            }

            AddMarker(marker.TrimEnd('/', '\\'));
        }

        void AddMarker(string marker, bool includePathPairs = true)
        {
            if (string.IsNullOrWhiteSpace(marker))
            {
                return;
            }

            markers.Add(marker);
            markers.Add(marker.Replace('\\', '/'));
            markers.Add(marker.Replace('/', '\\'));

            var slashMarker = marker.Replace('\\', '/');
            var backslashMarker = marker.Replace('/', '\\');
            if (includePathPairs && backslashMarker.StartsWith("\\\\?\\UNC\\", StringComparison.OrdinalIgnoreCase))
            {
                AddMarker("\\\\" + backslashMarker[8..], includePathPairs: false);
            }
            else if (includePathPairs && backslashMarker.StartsWith("\\\\?\\", StringComparison.OrdinalIgnoreCase))
            {
                AddMarker(backslashMarker[4..], includePathPairs: false);
            }
            else if (marker.Length >= 3 && char.IsAsciiLetter(marker[0]) &&
                marker[1] == ':' && (marker[2] == '\\' || marker[2] == '/'))
            {
                var verbatim = "\\\\?\\" + backslashMarker;
                markers.Add(verbatim);
                markers.Add("//?/" + slashMarker);
                markers.Add("file:///" + slashMarker);
                markers.Add("file:///" + EscapeUriPath(slashMarker));
            }
            else if (backslashMarker.StartsWith("\\\\", StringComparison.Ordinal))
            {
                var uncBody = backslashMarker.TrimStart('\\');
                if (includePathPairs)
                {
                    AddMarker("\\\\?\\UNC\\" + uncBody, includePathPairs: false);
                }

                markers.Add("file://" + uncBody.Replace('\\', '/'));
            }

            markers.Add(Uri.EscapeDataString(marker));
            markers.Add(Uri.EscapeDataString(slashMarker));
            if (Uri.TryCreate(marker, UriKind.Absolute, out var uri))
            {
                markers.Add(uri.AbsoluteUri);
            }
        }

        static string EscapeUriPath(string value)
        {
            return string.Join(
                '/',
                value.Split('/').Select(segment =>
                    Uri.EscapeDataString(segment).Replace("%3A", ":", StringComparison.OrdinalIgnoreCase)));
        }
    }

    private static bool ContainsEncodedHostPath(
        ReadOnlySpan<byte> bytes,
        IReadOnlyList<string> markers)
    {
        foreach (var marker in markers)
        {
            if (ContainsAsciiCaseInsensitive(bytes, Encoding.UTF8.GetBytes(marker)) ||
                ContainsAsciiCaseInsensitive(bytes, Encoding.Unicode.GetBytes(marker)))
            {
                return true;
            }
        }

        return false;
    }

    private static bool ContainsAsciiCaseInsensitive(
        ReadOnlySpan<byte> bytes,
        ReadOnlySpan<byte> marker)
    {
        if (marker.IsEmpty || marker.Length > bytes.Length)
        {
            return false;
        }

        for (var start = 0; start <= bytes.Length - marker.Length; start++)
        {
            var matched = true;
            for (var offset = 0; offset < marker.Length; offset++)
            {
                if (FoldAscii(bytes[start + offset]) != FoldAscii(marker[offset]))
                {
                    matched = false;
                    break;
                }
            }

            if (matched)
            {
                return true;
            }
        }

        return false;
    }

    private static byte FoldAscii(byte value)
    {
        return value is >= (byte)'A' and <= (byte)'Z'
            ? (byte)(value + ('a' - 'A'))
            : value;
    }

    private static void ValidateCompatibilityProjection(
        string? projection,
        byte[] authoritativeBytes,
        MspNativeOperation operation)
    {
        if (projection is null ||
            !string.Equals(
                projection,
                Encoding.UTF8.GetString(authoritativeBytes),
                StringComparison.Ordinal))
        {
            throw InvalidResponse(operation);
        }
    }

    private static bool IsFullyQualifiedWindowsHostPath(string path)
    {
        if (path.Contains('\0'))
        {
            return false;
        }

        if (OperatingSystem.IsWindows())
        {
            return Path.IsPathFullyQualified(path);
        }

        return path.Length >= 3 && char.IsAsciiLetter(path[0]) && path[1] == ':' &&
                (path[2] == '\\' || path[2] == '/') ||
            path.StartsWith("\\\\", StringComparison.Ordinal);
    }

    private static IEnumerable<string> EnumerateResultStrings(MspNativeCommandResult result)
    {
        yield return result.StdoutText;
        yield return result.StderrText;
        if (result.StateChange?.CurrentDirectory is { } currentDirectory)
        {
            yield return currentDirectory;
        }

        foreach (var diagnostic in result.Diagnostics)
        {
            foreach (var value in EnumerateDiagnosticStrings(diagnostic))
            {
                yield return value;
            }
        }

        foreach (var audit in result.AuditRecords)
        {
            yield return audit.RunId;
            yield return audit.CommandLine;
            yield return audit.CommandName ?? string.Empty;
            yield return audit.Actor;
            yield return audit.SessionId;
            yield return audit.WorkingDirectory;
            yield return audit.PolicyDecision.Reason ?? string.Empty;
            yield return audit.PolicyDecision.Prompt ?? string.Empty;
            foreach (var argument in audit.Arguments)
            {
                yield return argument;
            }

            foreach (var diagnostic in audit.Diagnostics)
            {
                foreach (var value in EnumerateDiagnosticStrings(diagnostic))
                {
                    yield return value;
                }
            }
        }
    }

    private static IEnumerable<string> EnumerateDiagnosticStrings(
        MspNativeDiagnostic diagnostic)
    {
        yield return diagnostic.Code;
        yield return diagnostic.Message;
        yield return diagnostic.Target ?? string.Empty;
        yield return diagnostic.RecoveryHint ?? string.Empty;
    }

    private static MspNativeShellParseResult FreezeParseResult(
        MspNativeShellParseResult result)
    {
        return result with
        {
            Script = result.Script is null
                ? null
                : result.Script with
                {
                    Pipelines = Array.AsReadOnly(
                        result.Script.Pipelines.Select(FreezePipeline).ToArray())
                },
            Error = result.Error is null ? null : result.Error with { }
        };
    }

    private static MspNativeParsedCommandPipeline FreezePipeline(
        MspNativeParsedCommandPipeline pipeline)
    {
        return pipeline with
        {
            Commands = Array.AsReadOnly(pipeline.Commands.Select(FreezeCommand).ToArray()),
            PipeOperators = Array.AsReadOnly(pipeline.PipeOperators.ToArray())
        };
    }

    private static MspNativeParsedCommandLine FreezeCommand(
        MspNativeParsedCommandLine command)
    {
        return command with
        {
            Arguments = Array.AsReadOnly(command.Arguments.ToArray()),
            Assignments = Array.AsReadOnly(
                command.Assignments.Select(assignment => assignment with { }).ToArray()),
            Redirections = Array.AsReadOnly(
                command.Redirections.Select(redirection => redirection with
                {
                    TargetWord = FreezeWord(redirection.TargetWord)
                }).ToArray()),
            CommandNameWord = command.CommandNameWord is null
                ? null
                : FreezeWord(command.CommandNameWord),
            ArgumentWords = Array.AsReadOnly(
                command.ArgumentWords.Select(FreezeWord).ToArray())
        };
    }

    private static MspNativeParsedWord FreezeWord(MspNativeParsedWord word)
    {
        return word with
        {
            Parts = Array.AsReadOnly(word.Parts.Select(part => part with { }).ToArray())
        };
    }

    private static MspNativeAdapterException InvalidResponse(MspNativeOperation operation)
    {
        return MspNativeAdapterException.Create(
            MspNativeFailureKind.InvalidResponse,
            operation);
    }

    private static bool IsFatal(Exception exception)
    {
        return exception is OutOfMemoryException or StackOverflowException;
    }

    private static JsonSerializerOptions CreateJsonOptions()
    {
        var options = new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
            PropertyNameCaseInsensitive = false,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };
        options.Converters.Add(new JsonStringEnumConverter(
            JsonNamingPolicy.CamelCase,
            allowIntegerValues: false));
        return options;
    }
}

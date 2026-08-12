using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.App.Services;

internal sealed class ReadOsPackageSmokeOptions
{
    public const string CommandLineSwitch = "--package-smoke";
    public const string NativeWorkspaceCommandLineSwitch = "--package-smoke-native-workspace";
    public const string NativeWorkspaceTemporaryDirectoryName = "ReadOS.PackageSmoke.Native";
    public const string MarkerFileName = "reados-package-smoke.success.json";
    public const string LogFileName = "reados-package-smoke.log.json";

    private ReadOsPackageSmokeOptions(string rootPath, string nativeWorkspaceRoot)
    {
        RootPath = rootPath;
        WorkspaceRoot = Path.Combine(rootPath, "workspace");
        CredentialRoot = Path.Combine(rootPath, "credentials");
        MarkerPath = Path.Combine(rootPath, MarkerFileName);
        LogPath = Path.Combine(rootPath, LogFileName);
        NativeWorkspaceRoot = nativeWorkspaceRoot;
    }

    public string RootPath { get; }

    public string WorkspaceRoot { get; }

    public string CredentialRoot { get; }

    public string MarkerPath { get; }

    public string LogPath { get; }

    public string NativeWorkspaceRoot { get; }

    public static bool IsRequested(IReadOnlyList<string> arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        return arguments.Any(argument =>
            string.Equals(argument, CommandLineSwitch, StringComparison.OrdinalIgnoreCase));
    }

    public static bool TryParse(
        IReadOnlyList<string> arguments,
        out ReadOsPackageSmokeOptions? options,
        out string? error)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        options = null;
        error = null;

        var switchIndexes = arguments
            .Select((argument, index) => (argument, index))
            .Where(item => string.Equals(
                item.argument,
                CommandLineSwitch,
                StringComparison.OrdinalIgnoreCase))
            .Select(item => item.index)
            .ToArray();
        if (switchIndexes.Length == 0)
        {
            return false;
        }

        if (switchIndexes.Length != 1)
        {
            error = $"{CommandLineSwitch} must be specified exactly once.";
            return false;
        }

        var switchIndex = switchIndexes[0];
        if (switchIndex + 1 >= arguments.Count ||
            string.IsNullOrWhiteSpace(arguments[switchIndex + 1]))
        {
            error = $"{CommandLineSwitch} requires an isolated smoke root path.";
            return false;
        }

        var nativeSwitchIndexes = arguments
            .Select((argument, index) => (argument, index))
            .Where(item => string.Equals(
                item.argument,
                NativeWorkspaceCommandLineSwitch,
                StringComparison.OrdinalIgnoreCase))
            .Select(item => item.index)
            .ToArray();
        if (nativeSwitchIndexes.Length != 1)
        {
            error = $"{NativeWorkspaceCommandLineSwitch} must be specified exactly once.";
            return false;
        }

        var nativeSwitchIndex = nativeSwitchIndexes[0];
        if (nativeSwitchIndex + 1 >= arguments.Count ||
            string.IsNullOrWhiteSpace(arguments[nativeSwitchIndex + 1]))
        {
            error = $"{NativeWorkspaceCommandLineSwitch} requires an isolated native workspace path.";
            return false;
        }

        try
        {
            var requestedRootPath = arguments[switchIndex + 1];
            if (!Path.IsPathFullyQualified(requestedRootPath))
            {
                error = "The package smoke root must be an absolute path.";
                return false;
            }

            var rootPath = Path.GetFullPath(requestedRootPath);
            if (!HasArtifactsAncestor(rootPath))
            {
                error = "The package smoke root must be a child of an artifacts directory.";
                return false;
            }

            var requestedNativeWorkspaceRoot = arguments[nativeSwitchIndex + 1];
            if (!Path.IsPathFullyQualified(requestedNativeWorkspaceRoot))
            {
                error = "The native package smoke workspace root must be an absolute path.";
                return false;
            }

            var nativeWorkspaceRoot = Path.GetFullPath(requestedNativeWorkspaceRoot);
            var nativeWorkspaceTemporaryParent = Path.GetFullPath(Path.Combine(
                Path.GetTempPath(),
                NativeWorkspaceTemporaryDirectoryName));
            if (!IsStrictDescendant(
                nativeWorkspaceTemporaryParent,
                nativeWorkspaceRoot))
            {
                error = "The native package smoke workspace root must be below its dedicated temporary directory.";
                return false;
            }

            if (!AreSeparated(rootPath, nativeWorkspaceRoot))
            {
                error = "The native package smoke workspace root must be separate from the managed smoke root.";
                return false;
            }

            options = new ReadOsPackageSmokeOptions(rootPath, nativeWorkspaceRoot);
            return true;
        }
        catch (Exception ex) when (ex is ArgumentException or NotSupportedException or PathTooLongException)
        {
            error = $"The package smoke root is invalid: {ex.Message}";
            return false;
        }
    }

    private static bool AreSeparated(string firstPath, string secondPath)
    {
        return !IsSameOrDescendant(firstPath, secondPath) &&
            !IsSameOrDescendant(secondPath, firstPath);
    }

    private static bool IsStrictDescendant(string parentPath, string candidatePath)
    {
        return !string.Equals(
                Path.GetFullPath(parentPath).TrimEnd(
                    Path.DirectorySeparatorChar,
                    Path.AltDirectorySeparatorChar),
                Path.GetFullPath(candidatePath).TrimEnd(
                    Path.DirectorySeparatorChar,
                    Path.AltDirectorySeparatorChar),
                StringComparison.OrdinalIgnoreCase) &&
            IsSameOrDescendant(parentPath, candidatePath);
    }

    private static bool IsSameOrDescendant(string parentPath, string candidatePath)
    {
        var normalizedParent = Path.GetFullPath(parentPath).TrimEnd(
            Path.DirectorySeparatorChar,
            Path.AltDirectorySeparatorChar);
        var normalizedCandidate = Path.GetFullPath(candidatePath).TrimEnd(
            Path.DirectorySeparatorChar,
            Path.AltDirectorySeparatorChar);
        if (string.Equals(
            normalizedParent,
            normalizedCandidate,
            StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        return normalizedCandidate.StartsWith(
            normalizedParent + Path.DirectorySeparatorChar,
            StringComparison.OrdinalIgnoreCase);
    }

    private static bool HasArtifactsAncestor(string path)
    {
        var parent = Directory.GetParent(path);
        while (parent is not null)
        {
            if (string.Equals(parent.Name, "artifacts", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }

            parent = parent.Parent;
        }

        return false;
    }
}

internal static class ReadOsPackageSmokeComposition
{
    public static ServiceProvider Build(
        ReadOsPackageSmokeOptions options,
        ICurrentUserDataProtector? dataProtector = null,
        IMspNativeAdapter? nativeAdapter = null)
    {
        ArgumentNullException.ThrowIfNull(options);

        var services = new ServiceCollection();
        services.AddSingleton(options);
        services.AddSingleton<IPdfDocumentService, PdfDocumentService>();
        services.AddSingleton<IProviderCredentialStore>(_ =>
            new WindowsDpapiProviderCredentialStore(
                options.CredentialRoot,
                dataProtector ?? new DpapiCurrentUserDataProtector()));
        services.AddSingleton<IWorkspaceStore>(provider =>
            new WorkspaceStore(
                provider.GetRequiredService<IPdfDocumentService>(),
                provider.GetRequiredService<IProviderCredentialStore>(),
                options.WorkspaceRoot));
        services.AddSingleton<IAiChatService, AiChatService>();
        services.AddSingleton<IMspNativeAdapter>(_ =>
            nativeAdapter ?? MspNativeAdapter.LoadPackagedWindows());
        services.AddSingleton<ReadOsMspHostingReadinessService>();
        services.AddSingleton<ReadOsPackageSmokeService>();

        return services.BuildServiceProvider(new ServiceProviderOptions
        {
            ValidateOnBuild = true,
            ValidateScopes = true
        });
    }
}

internal sealed class ReadOsPackageSmokeService
{
    private const string CredentialScope = "https://package-smoke.reados.invalid/v1";
    private const uint RequiredNativeAbiMajor = 2;
    private const uint RequiredNativeAbiMinor = 0;
    private const ulong RequiredNativeAbiContractId = 0x324D534F44414552UL;
    private const ulong RequiredNativeAbiCapabilities = 0x000000000000000FUL;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true
    };

    private readonly ReadOsPackageSmokeOptions options;
    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly IProviderCredentialStore credentialStore;
    private readonly IAiChatService aiChatService;
    private readonly IMspNativeAdapter nativeAdapter;
    private readonly ReadOsMspHostingReadinessService readinessService;

    public ReadOsPackageSmokeService(
        ReadOsPackageSmokeOptions options,
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        IProviderCredentialStore credentialStore,
        IAiChatService aiChatService,
        IMspNativeAdapter nativeAdapter,
        ReadOsMspHostingReadinessService readinessService)
    {
        this.options = options ?? throw new ArgumentNullException(nameof(options));
        this.workspaceStore = workspaceStore ?? throw new ArgumentNullException(nameof(workspaceStore));
        this.pdfService = pdfService ?? throw new ArgumentNullException(nameof(pdfService));
        this.credentialStore = credentialStore ?? throw new ArgumentNullException(nameof(credentialStore));
        this.aiChatService = aiChatService ?? throw new ArgumentNullException(nameof(aiChatService));
        this.nativeAdapter = nativeAdapter ?? throw new ArgumentNullException(nameof(nativeAdapter));
        this.readinessService = readinessService ?? throw new ArgumentNullException(nameof(readinessService));
    }

    public async Task<int> RunAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            Directory.CreateDirectory(options.RootPath);
            TryDelete(options.MarkerPath);
            AssertPathEquals(workspaceStore.WorkspaceRoot, options.WorkspaceRoot, "workspace root");
            var workspace = await workspaceStore.LoadAsync(cancellationToken);
            if (!string.IsNullOrEmpty(workspace.Settings.ProviderApiKey))
            {
                throw new InvalidOperationException(
                    "The isolated smoke workspace unexpectedly loaded a provider credential.");
            }

            await VerifyCredentialRoundTripAsync(cancellationToken);
            await workspaceStore.SaveAsync(workspace, cancellationToken);
            await VerifyWorkspaceStateAsync(cancellationToken);
            var nativeRuntimeInfo = RequireNativeAbiV2();
            PrepareNativeWorkspaceFixture();

            var nativeList = nativeAdapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "ls /",
                WorkingDirectory = "/",
                Actor = "package-smoke",
                SessionId = "package-smoke-native",
                WorkspaceRoot = options.NativeWorkspaceRoot
            });
            if (!nativeList.Succeeded ||
                !nativeList.StdoutText
                    .Split('\n', StringSplitOptions.RemoveEmptyEntries)
                    .Contains("workspace.json", StringComparer.Ordinal))
            {
                throw new InvalidOperationException(
                    "The packaged Rust MSP runtime did not list the isolated workspace.");
            }

            var nativeCat = nativeAdapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "cat /workspace.json",
                WorkingDirectory = "/",
                Actor = "package-smoke",
                SessionId = "package-smoke-native",
                WorkspaceRoot = options.NativeWorkspaceRoot
            });
            if (!nativeCat.Succeeded || nativeCat.StdoutBytes.IsEmpty)
            {
                throw new InvalidOperationException(
                    "The packaged Rust MSP runtime did not read workspace.json.");
            }
            if (nativeCat.StdoutText.Contains(
                    "providerApiKey",
                    StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException(
                    "The packaged Rust MSP runtime exposed a provider API key field.");
            }

            var readiness = readinessService.BuildReport();
            if (readiness.Boundaries.Count == 0 ||
                !readiness.Boundaries.Any(boundary =>
                    boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted))
            {
                throw new InvalidOperationException(
                    "MSP hosting readiness did not report any hosted boundary.");
            }

            using var hostNativeAdapterProvider = new LazyMspNativeAdapterProvider(
                () => nativeAdapter);
            using var hostRuntime = new ReadOsMspHostRuntimeFactory().Create(
                CreateHostDependencies(workspace),
                ReadOsMspHost.DefaultSessionId,
                "package-smoke",
                hostNativeAdapterProvider);
            if (!hostRuntime.Diagnostics.HostCommandNames.Contains(
                "workspace",
                StringComparer.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException(
                    "The package smoke MSP host did not compose the workspace command.");
            }

            var commandResult = await hostRuntime.CommandHost.ExecuteAsync(
                hostRuntime.RequestFactory.Create("workspace info"),
                cancellationToken);
            if (!commandResult.Succeeded)
            {
                throw new InvalidOperationException(
                    $"The package smoke MSP command failed with exit code {commandResult.ExitCode}: {commandResult.Stderr}");
            }

            if (!commandResult.Stdout.Contains(
                "workspaceRoot\t<workspace>",
                StringComparison.OrdinalIgnoreCase) ||
                !commandResult.Stdout.Contains(
                    "libraryRoot\t<library>",
                    StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException(
                    "The package smoke MSP host did not use the isolated workspace root.");
            }

            var nativePwdResult = await hostRuntime.CommandHost.ExecuteAsync(
                hostRuntime.RequestFactory.Create("pwd"),
                cancellationToken);
            if (!nativePwdResult.Succeeded ||
                !string.Equals(nativePwdResult.Stdout, "/\n", StringComparison.Ordinal) ||
                nativePwdResult.AuditRecords.Count != 1 ||
                !string.Equals(
                    nativePwdResult.AuditRecords[0].CommandName,
                    "pwd",
                    StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "The package smoke MSP host did not execute pwd through the Rust proxy.");
            }

            var nativeEchoResult = await hostRuntime.CommandHost.ExecuteAsync(
                hostRuntime.RequestFactory.Create("echo ''"),
                cancellationToken);
            if (!nativeEchoResult.Succeeded ||
                !string.Equals(nativeEchoResult.Stdout, "\n", StringComparison.Ordinal) ||
                nativeEchoResult.AuditRecords.Count != 1 ||
                !string.Equals(
                    nativeEchoResult.AuditRecords[0].CommandName,
                    "echo",
                    StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "The package smoke MSP host did not preserve an explicit empty echo argument through the Rust proxy.");
            }

            const string nativeEchoMarker = "reados-native-proxy";
            var nativeEchoMarkerResult = await hostRuntime.CommandHost.ExecuteAsync(
                hostRuntime.RequestFactory.Create($"echo -n {nativeEchoMarker}"),
                cancellationToken);
            if (!nativeEchoMarkerResult.Succeeded ||
                !string.Equals(
                    nativeEchoMarkerResult.Stdout,
                    nativeEchoMarker,
                    StringComparison.Ordinal) ||
                nativeEchoMarkerResult.AuditRecords.Count != 1)
            {
                throw new InvalidOperationException(
                    "The package smoke MSP host did not execute the marker echo through the Rust proxy.");
            }

            var completedAt = DateTimeOffset.UtcNow;
            await WriteJsonAtomicallyAsync(
                options.LogPath,
                new
                {
                    schemaVersion = 1,
                    status = "ready",
                    completedAt,
                    processId = Environment.ProcessId,
                    workspaceRoot = options.WorkspaceRoot,
                    credentialRoot = options.CredentialRoot,
                    readiness.CurrentHost,
                    readiness.PlannedHost,
                    hostedBoundaryCount = readiness.Boundaries.Count(boundary =>
                        boundary.Status == ReadOsMspHostingBoundaryStatus.Hosted),
                    commandCount = hostRuntime.Diagnostics.CommandCount,
                    hostCommands = hostRuntime.Diagnostics.HostCommandNames,
                    command = "workspace info",
                    commandExitCode = commandResult.ExitCode,
                    nativeContractVersion = nativeList.ContractVersion,
                    nativeAbiMode = nativeRuntimeInfo.AbiMode.ToString(),
                    nativeAbiMajor = nativeRuntimeInfo.MajorVersion,
                    nativeAbiMinor = nativeRuntimeInfo.MinorVersion,
                    nativeAbiContractId = $"0x{nativeRuntimeInfo.ContractId:X16}",
                    nativeAbiCapabilities = $"0x{nativeRuntimeInfo.Capabilities:X16}",
                    nativeCommands = new[]
                    {
                        "ls /",
                        "cat /workspace.json",
                        "pwd",
                        "echo ''",
                        $"echo -n {nativeEchoMarker}"
                    },
                    nativeListExitCode = nativeList.ExitCode,
                    nativeCatExitCode = nativeCat.ExitCode,
                    nativeCatBytes = nativeCat.StdoutBytes.Length,
                    nativePwdExitCode = nativePwdResult.ExitCode,
                    nativeEchoExitCode = nativeEchoResult.ExitCode,
                    nativeEchoMarkerExitCode = nativeEchoMarkerResult.ExitCode,
                    nativePwdAuditCount = nativePwdResult.AuditRecords.Count,
                    nativeEchoAuditCount = nativeEchoResult.AuditRecords.Count,
                    nativeEchoMarkerAuditCount = nativeEchoMarkerResult.AuditRecords.Count
                },
                cancellationToken);
            await WriteJsonAtomicallyAsync(
                options.MarkerPath,
                new
                {
                    schemaVersion = 1,
                    status = "ready",
                    completedAt,
                    processId = Environment.ProcessId
                },
                cancellationToken);
            return 0;
        }
        catch (Exception ex)
        {
            TryDelete(options.MarkerPath);
            try
            {
                await WriteJsonAtomicallyAsync(
                    options.LogPath,
                    new
                    {
                        schemaVersion = 1,
                        status = "failed",
                        failedAt = DateTimeOffset.UtcNow,
                        processId = Environment.ProcessId,
                        errorType = ex.GetType().FullName,
                        error = RedactNativeWorkspaceRoot(ex.Message),
                        details = RedactNativeWorkspaceRoot(ex.ToString())
                    },
                    CancellationToken.None);
            }
            catch
            {
                // The process exit code and missing marker still make the failure observable
                // when the smoke root itself cannot be written.
            }

            return 1;
        }
    }

    private MspNativeRuntimeInfo RequireNativeAbiV2()
    {
        if (nativeAdapter is not IMspNativeRuntimeInfoProvider infoProvider)
        {
            throw new InvalidOperationException(
                "The packaged Rust MSP adapter did not expose native ABI handshake evidence.");
        }

        var info = infoProvider.NativeRuntimeInfo;
        if (info.AbiMode != MspNativeAbiMode.LengthDelimitedV2 ||
            info.MajorVersion != RequiredNativeAbiMajor ||
            info.MinorVersion != RequiredNativeAbiMinor ||
            info.ContractId != RequiredNativeAbiContractId ||
            (info.Capabilities & RequiredNativeAbiCapabilities) !=
                RequiredNativeAbiCapabilities)
        {
            throw new InvalidOperationException(
                "The packaged Rust MSP adapter did not negotiate the required length-delimited ABI v2 contract.");
        }

        return info;
    }

    private void PrepareNativeWorkspaceFixture()
    {
        try
        {
            var nativeWorkspaceTemporaryParent = Path.GetFullPath(Path.Combine(
                Path.GetTempPath(),
                ReadOsPackageSmokeOptions.NativeWorkspaceTemporaryDirectoryName));
            if (!Directory.Exists(options.NativeWorkspaceRoot) ||
                HasReparsePointBoundary(
                    options.NativeWorkspaceRoot,
                    nativeWorkspaceTemporaryParent) ||
                Directory.EnumerateFileSystemEntries(options.NativeWorkspaceRoot).Any())
            {
                throw new InvalidOperationException();
            }

            File.Copy(
                Path.Combine(options.WorkspaceRoot, "workspace.json"),
                Path.Combine(options.NativeWorkspaceRoot, "workspace.json"));
        }
        catch (Exception ex) when (ex is IOException or
            UnauthorizedAccessException or
            InvalidOperationException or
            NotSupportedException or
            PathTooLongException)
        {
            throw new InvalidOperationException(
                "The isolated native workspace fixture could not be prepared.");
        }
    }

    private static bool HasReparsePointBoundary(string path, string expectedParent)
    {
        var normalizedExpectedParent = Path.GetFullPath(expectedParent).TrimEnd(
            Path.DirectorySeparatorChar,
            Path.AltDirectorySeparatorChar);
        var current = new DirectoryInfo(path);
        while (current is not null)
        {
            if ((current.Attributes & FileAttributes.ReparsePoint) != 0)
            {
                return true;
            }

            if (string.Equals(
                current.FullName.TrimEnd(
                    Path.DirectorySeparatorChar,
                    Path.AltDirectorySeparatorChar),
                normalizedExpectedParent,
                StringComparison.OrdinalIgnoreCase))
            {
                return false;
            }

            current = current.Parent;
        }

        return true;
    }

    private ReadOsMspHostDependencies CreateHostDependencies(WorkspaceState workspace)
    {
        return new ReadOsMspHostDependencies(
            workspaceStore,
            pdfService,
            aiChatService,
            () => workspace,
            () => workspace.Settings,
            () => null,
            () => Array.Empty<ChatAttachment>(),
            _ => Task.FromResult(string.Empty),
            _ => { },
            () => { },
            (_, _) => { });
    }

    private async Task VerifyCredentialRoundTripAsync(CancellationToken cancellationToken)
    {
        var credentialBytes = RandomNumberGenerator.GetBytes(24);
        string ephemeralCredential;
        try
        {
            ephemeralCredential = Convert.ToHexString(credentialBytes);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(credentialBytes);
        }

        try
        {
            await credentialStore.SetApiKeyAsync(
                CredentialScope,
                ephemeralCredential,
                cancellationToken);
            var restoredCredential = await credentialStore.GetApiKeyAsync(
                CredentialScope,
                cancellationToken);
            if (!string.Equals(
                ephemeralCredential,
                restoredCredential,
                StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "The isolated DPAPI credential round trip did not preserve the value.");
            }
        }
        finally
        {
            await credentialStore.SetApiKeyAsync(
                CredentialScope,
                null,
                CancellationToken.None);
        }

        if (await credentialStore.GetApiKeyAsync(CredentialScope, cancellationToken) is not null)
        {
            throw new InvalidOperationException(
                "The isolated DPAPI credential was not removed after verification.");
        }
    }

    private async Task VerifyWorkspaceStateAsync(CancellationToken cancellationToken)
    {
        var statePath = Path.Combine(options.WorkspaceRoot, "workspace.json");
        if (!File.Exists(statePath))
        {
            throw new InvalidOperationException(
                "The isolated WorkspaceStore did not persist workspace.json.");
        }

        var persistedState = await File.ReadAllTextAsync(statePath, cancellationToken);
        if (persistedState.Contains("providerApiKey", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "The isolated WorkspaceStore persisted a provider API key field.");
        }
    }

    private static void AssertPathEquals(string actual, string expected, string description)
    {
        if (!string.Equals(
            Path.GetFullPath(actual).TrimEnd(Path.DirectorySeparatorChar),
            Path.GetFullPath(expected).TrimEnd(Path.DirectorySeparatorChar),
            StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                $"The package smoke {description} is not isolated under its smoke root.");
        }
    }

    private string RedactNativeWorkspaceRoot(string value)
    {
        var redacted = value;
        var normalizedRoot = Path.GetFullPath(options.NativeWorkspaceRoot).TrimEnd(
            Path.DirectorySeparatorChar,
            Path.AltDirectorySeparatorChar);
        var variants = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            normalizedRoot,
            normalizedRoot.Replace('\\', '/'),
            @"\\?\" + normalizedRoot,
            Uri.EscapeDataString(normalizedRoot)
        };
        if (Uri.TryCreate(normalizedRoot, UriKind.Absolute, out var rootUri))
        {
            variants.Add(rootUri.AbsoluteUri.TrimEnd('/'));
        }

        foreach (var variant in variants.OrderByDescending(item => item.Length))
        {
            redacted = redacted.Replace(
                variant,
                "[native-workspace]",
                StringComparison.OrdinalIgnoreCase);
        }

        return redacted;
    }

    private static async Task WriteJsonAtomicallyAsync(
        string path,
        object value,
        CancellationToken cancellationToken)
    {
        var directory = Path.GetDirectoryName(path)
            ?? throw new InvalidOperationException("Smoke output path has no parent directory.");
        Directory.CreateDirectory(directory);
        var tempPath = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            await File.WriteAllTextAsync(
                tempPath,
                JsonSerializer.Serialize(value, JsonOptions),
                cancellationToken);
            File.Move(tempPath, path, overwrite: true);
        }
        finally
        {
            TryDelete(tempPath);
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch
        {
            // A later atomic write or the script-level cleanup reports the actionable error.
        }
    }
}

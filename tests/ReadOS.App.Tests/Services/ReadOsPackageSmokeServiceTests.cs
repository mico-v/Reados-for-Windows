using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using ReadOS.App.Services;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.App.Tests.Services;

public sealed class ReadOsPackageSmokeServiceTests
{
    [Fact]
    public void Options_require_an_absolute_root_below_artifacts()
    {
        Assert.False(ReadOsPackageSmokeOptions.TryParse(
            Array.Empty<string>(),
            out var missingOptions,
            out var missingError));
        Assert.Null(missingOptions);
        Assert.Null(missingError);

        Assert.False(ReadOsPackageSmokeOptions.TryParse(
            new[]
            {
                ReadOsPackageSmokeOptions.CommandLineSwitch,
                @"artifacts\smoke\run",
                ReadOsPackageSmokeOptions.NativeWorkspaceCommandLineSwitch,
                Path.Combine(Path.GetTempPath(), "reados-native-smoke")
            },
            out var relativeOptions,
            out var relativeError));
        Assert.Null(relativeOptions);
        Assert.Contains("absolute", relativeError);

        Assert.False(ReadOsPackageSmokeOptions.TryParse(
            new[]
            {
                ReadOsPackageSmokeOptions.CommandLineSwitch,
                Path.Combine(Path.GetTempPath(), "artifacts", "smoke", "run"),
                ReadOsPackageSmokeOptions.NativeWorkspaceCommandLineSwitch,
                Path.Combine(Path.GetTempPath(), "outside-native-smoke")
            },
            out var outsideTemporaryParentOptions,
            out var outsideTemporaryParentError));
        Assert.Null(outsideTemporaryParentOptions);
        Assert.Contains("dedicated temporary directory", outsideTemporaryParentError);

        Assert.False(ReadOsPackageSmokeOptions.TryParse(
            new[]
            {
                ReadOsPackageSmokeOptions.CommandLineSwitch,
                Path.Combine(Path.GetTempPath(), "artifacts", "smoke", "run"),
                ReadOsPackageSmokeOptions.NativeWorkspaceCommandLineSwitch,
                @"native-workspace"
            },
            out var relativeNativeOptions,
            out var relativeNativeError));
        Assert.Null(relativeNativeOptions);
        Assert.Contains("absolute", relativeNativeError);

        using var directory = new TemporaryArtifactsDirectory();
        Assert.True(ReadOsPackageSmokeOptions.TryParse(
            new[]
            {
                ReadOsPackageSmokeOptions.CommandLineSwitch,
                directory.SmokeRoot,
                ReadOsPackageSmokeOptions.NativeWorkspaceCommandLineSwitch,
                directory.NativeWorkspaceRoot
            },
            out var options,
            out var error),
            error);

        Assert.NotNull(options);
        Assert.Equal(Path.GetFullPath(directory.SmokeRoot), options.RootPath);
        Assert.Equal(
            Path.Combine(directory.SmokeRoot, "workspace"),
            options.WorkspaceRoot);
        Assert.Equal(
            Path.Combine(directory.SmokeRoot, "credentials"),
            options.CredentialRoot);
        Assert.Equal(
            Path.Combine(directory.SmokeRoot, ReadOsPackageSmokeOptions.MarkerFileName),
            options.MarkerPath);
        Assert.Equal(
            Path.Combine(directory.SmokeRoot, ReadOsPackageSmokeOptions.LogFileName),
            options.LogPath);
        Assert.Equal(
            Path.GetFullPath(directory.NativeWorkspaceRoot),
            options.NativeWorkspaceRoot);

        var overlappingSmokeRoot = Path.Combine(
            directory.NativeRunRoot,
            "artifacts",
            "smoke",
            "package");
        Assert.False(ReadOsPackageSmokeOptions.TryParse(
            new[]
            {
                ReadOsPackageSmokeOptions.CommandLineSwitch,
                overlappingSmokeRoot,
                ReadOsPackageSmokeOptions.NativeWorkspaceCommandLineSwitch,
                Path.Combine(overlappingSmokeRoot, "native-workspace")
            },
            out var overlappingOptions,
            out var overlappingError));
        Assert.Null(overlappingOptions);
        Assert.Contains("separate", overlappingError);
    }

    [Fact]
    public async Task Runner_initializes_isolated_workspace_credentials_and_msp_host()
    {
        using var directory = new TemporaryArtifactsDirectory();
        var options = ParseOptions(directory.SmokeRoot, directory.NativeWorkspaceRoot);
        await using var services = ReadOsPackageSmokeComposition.Build(
            options,
            new ReversibleTestDataProtector(),
            CreateNativeAdapter(options.NativeWorkspaceRoot));

        var exitCode = await services
            .GetRequiredService<ReadOsPackageSmokeService>()
            .RunAsync();

        Assert.True(
            exitCode == 0,
            File.Exists(options.LogPath)
                ? await File.ReadAllTextAsync(options.LogPath)
                : "The package smoke did not write a diagnostic log.");
        Assert.True(File.Exists(options.MarkerPath));
        Assert.True(File.Exists(options.LogPath));
        Assert.True(File.Exists(Path.Combine(options.WorkspaceRoot, "workspace.json")));

        using (var marker = JsonDocument.Parse(await File.ReadAllTextAsync(options.MarkerPath)))
        {
            Assert.Equal("ready", marker.RootElement.GetProperty("status").GetString());
            Assert.Equal(1, marker.RootElement.GetProperty("schemaVersion").GetInt32());
        }

        using (var log = JsonDocument.Parse(await File.ReadAllTextAsync(options.LogPath)))
        {
            Assert.Equal("ready", log.RootElement.GetProperty("status").GetString());
            Assert.True(log.RootElement.GetProperty("hostedBoundaryCount").GetInt32() > 0);
            Assert.True(log.RootElement.GetProperty("commandCount").GetInt32() > 0);
            Assert.Contains(
                log.RootElement.GetProperty("hostCommands").EnumerateArray(),
                command => string.Equals(
                    command.GetString(),
                    "workspace",
                    StringComparison.OrdinalIgnoreCase));
            Assert.Equal(0, log.RootElement.GetProperty("commandExitCode").GetInt32());
            Assert.Equal(
                MspNativeContract.Version,
                log.RootElement.GetProperty("nativeContractVersion").GetString());
            Assert.Equal(
                MspNativeAbiMode.LengthDelimitedV2.ToString(),
                log.RootElement.GetProperty("nativeAbiMode").GetString());
            Assert.Equal(2U, log.RootElement.GetProperty("nativeAbiMajor").GetUInt32());
            Assert.Equal(0U, log.RootElement.GetProperty("nativeAbiMinor").GetUInt32());
            Assert.Equal(
                "0x324D534F44414552",
                log.RootElement.GetProperty("nativeAbiContractId").GetString());
            Assert.Equal(
                "0x000000000000000F",
                log.RootElement.GetProperty("nativeAbiCapabilities").GetString());
            Assert.Equal(0, log.RootElement.GetProperty("nativeListExitCode").GetInt32());
            Assert.Equal(0, log.RootElement.GetProperty("nativeCatExitCode").GetInt32());
            Assert.True(log.RootElement.GetProperty("nativeCatBytes").GetInt32() > 0);
            Assert.Equal(0, log.RootElement.GetProperty("nativePwdExitCode").GetInt32());
            Assert.Equal(0, log.RootElement.GetProperty("nativeEchoExitCode").GetInt32());
            Assert.Equal(0, log.RootElement.GetProperty("nativeEchoMarkerExitCode").GetInt32());
            Assert.Equal(1, log.RootElement.GetProperty("nativePwdAuditCount").GetInt32());
            Assert.Equal(1, log.RootElement.GetProperty("nativeEchoAuditCount").GetInt32());
            Assert.Equal(1, log.RootElement.GetProperty("nativeEchoMarkerAuditCount").GetInt32());
            Assert.Contains(
                log.RootElement.GetProperty("nativeCommands").EnumerateArray(),
                command => string.Equals(
                    command.GetString(),
                    "echo -n reados-native-proxy",
                    StringComparison.Ordinal));
            Assert.DoesNotContain(
                options.NativeWorkspaceRoot,
                log.RootElement.GetRawText(),
                StringComparison.OrdinalIgnoreCase);
        }

        var serializedWorkspace = await File.ReadAllTextAsync(
            Path.Combine(options.WorkspaceRoot, "workspace.json"));
        Assert.DoesNotContain("providerApiKey", serializedWorkspace, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(
            Path.GetFullPath(options.WorkspaceRoot),
            services.GetRequiredService<IWorkspaceStore>().WorkspaceRoot);
        Assert.Empty(Directory.EnumerateFiles(options.CredentialRoot));
        Assert.True(File.Exists(Path.Combine(
            options.NativeWorkspaceRoot,
            "workspace.json")));
    }

    [Fact]
    public async Task Runner_rejects_an_adapter_without_the_v2_handshake()
    {
        using var directory = new TemporaryArtifactsDirectory();
        var options = ParseOptions(directory.SmokeRoot, directory.NativeWorkspaceRoot);
        var nativeTransport = new SmokeNativeTransport(
            options.NativeWorkspaceRoot,
            MspNativeRuntimeInfo.Unknown);
        await using var services = ReadOsPackageSmokeComposition.Build(
            options,
            new ReversibleTestDataProtector(),
            new MspNativeAdapter(nativeTransport));

        var exitCode = await services
            .GetRequiredService<ReadOsPackageSmokeService>()
            .RunAsync();

        Assert.Equal(1, exitCode);
        Assert.Equal(0, nativeTransport.InvokeCount);
        Assert.False(File.Exists(options.MarkerPath));
        using var log = JsonDocument.Parse(await File.ReadAllTextAsync(options.LogPath));
        Assert.Equal("failed", log.RootElement.GetProperty("status").GetString());
        Assert.Contains(
            "did not negotiate the required length-delimited ABI v2 contract",
            log.RootElement.GetProperty("details").GetString(),
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            options.NativeWorkspaceRoot,
            log.RootElement.GetRawText(),
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task Runner_returns_nonzero_and_writes_diagnostics_without_success_marker()
    {
        using var directory = new TemporaryArtifactsDirectory();
        var options = ParseOptions(directory.SmokeRoot, directory.NativeWorkspaceRoot);
        await using var services = ReadOsPackageSmokeComposition.Build(
            options,
            new ThrowingTestDataProtector(),
            CreateNativeAdapter(options.NativeWorkspaceRoot));

        var exitCode = await services
            .GetRequiredService<ReadOsPackageSmokeService>()
            .RunAsync();

        Assert.Equal(1, exitCode);
        Assert.False(File.Exists(options.MarkerPath));
        Assert.True(File.Exists(options.LogPath));
        using var log = JsonDocument.Parse(await File.ReadAllTextAsync(options.LogPath));
        Assert.Equal("failed", log.RootElement.GetProperty("status").GetString());
        Assert.Contains(
            "synthetic credential protection failure",
            log.RootElement.GetProperty("details").GetString(),
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            options.NativeWorkspaceRoot,
            log.RootElement.GetRawText(),
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task Runner_refuses_to_overwrite_a_nonempty_native_workspace()
    {
        using var directory = new TemporaryArtifactsDirectory();
        var options = ParseOptions(directory.SmokeRoot, directory.NativeWorkspaceRoot);
        var existingFixturePath = Path.Combine(
            options.NativeWorkspaceRoot,
            "workspace.json");
        const string existingFixture = "do-not-overwrite";
        await File.WriteAllTextAsync(existingFixturePath, existingFixture);
        await using var services = ReadOsPackageSmokeComposition.Build(
            options,
            new ReversibleTestDataProtector(),
            CreateNativeAdapter(options.NativeWorkspaceRoot));

        var exitCode = await services
            .GetRequiredService<ReadOsPackageSmokeService>()
            .RunAsync();

        Assert.Equal(1, exitCode);
        Assert.Equal(existingFixture, await File.ReadAllTextAsync(existingFixturePath));
        Assert.False(File.Exists(options.MarkerPath));
        using var log = JsonDocument.Parse(await File.ReadAllTextAsync(options.LogPath));
        Assert.Equal("failed", log.RootElement.GetProperty("status").GetString());
        Assert.Contains(
            "could not be prepared",
            log.RootElement.GetProperty("error").GetString(),
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            options.NativeWorkspaceRoot,
            log.RootElement.GetRawText(),
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task Runner_redacts_the_native_workspace_from_failure_diagnostics()
    {
        using var directory = new TemporaryArtifactsDirectory();
        var options = ParseOptions(directory.SmokeRoot, directory.NativeWorkspaceRoot);
        await using var services = ReadOsPackageSmokeComposition.Build(
            options,
            new ReversibleTestDataProtector(),
            new PathLeakingNativeAdapter(options.NativeWorkspaceRoot));

        var exitCode = await services
            .GetRequiredService<ReadOsPackageSmokeService>()
            .RunAsync();

        Assert.Equal(1, exitCode);
        Assert.False(File.Exists(options.MarkerPath));
        using var log = JsonDocument.Parse(await File.ReadAllTextAsync(options.LogPath));
        Assert.Equal("failed", log.RootElement.GetProperty("status").GetString());
        Assert.Contains(
            "[native-workspace]",
            log.RootElement.GetProperty("details").GetString(),
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            options.NativeWorkspaceRoot,
            log.RootElement.GetRawText(),
            StringComparison.OrdinalIgnoreCase);
    }

    private static ReadOsPackageSmokeOptions ParseOptions(
        string rootPath,
        string nativeWorkspaceRoot)
    {
        Assert.True(ReadOsPackageSmokeOptions.TryParse(
            new[]
            {
                ReadOsPackageSmokeOptions.CommandLineSwitch,
                rootPath,
                ReadOsPackageSmokeOptions.NativeWorkspaceCommandLineSwitch,
                nativeWorkspaceRoot
            },
            out var options,
            out var error),
            error);
        return Assert.IsType<ReadOsPackageSmokeOptions>(options);
    }

    private static MspNativeAdapter CreateNativeAdapter(
        string expectedNativeWorkspaceRoot)
    {
        return new MspNativeAdapter(new SmokeNativeTransport(
            expectedNativeWorkspaceRoot,
            new MspNativeRuntimeInfo(
                MspNativeAbiMode.LengthDelimitedV2,
                2,
                0,
                0x324D534F44414552UL,
                0x000000000000000FUL)));
    }

    private sealed class SmokeNativeTransport :
        IMspNativeTransport,
        IMspNativeRuntimeInfoProvider
    {
        private readonly string expectedNativeWorkspaceRoot;

        public SmokeNativeTransport(
            string expectedNativeWorkspaceRoot,
            MspNativeRuntimeInfo nativeRuntimeInfo)
        {
            this.expectedNativeWorkspaceRoot = expectedNativeWorkspaceRoot;
            NativeRuntimeInfo = nativeRuntimeInfo;
        }

        public MspNativeRuntimeInfo NativeRuntimeInfo { get; }

        public int InvokeCount { get; private set; }

        public byte[] Invoke(
            MspNativeOperation operation,
            ReadOnlyMemory<byte> requestJsonUtf8)
        {
            InvokeCount++;
            using var request = JsonDocument.Parse(requestJsonUtf8);
            var commandText = request.RootElement
                .GetProperty("commandText")
                .GetString();
            Assert.NotNull(commandText);

            return operation switch
            {
                MspNativeOperation.Parse => CreateParseResponse(commandText),
                MspNativeOperation.Execute => CreateExecuteResponse(
                    request.RootElement,
                    commandText),
                _ => throw new InvalidOperationException(
                    $"Unexpected native smoke operation: {operation}")
            };
        }

        private byte[] CreateExecuteResponse(JsonElement request, string commandText)
        {
            var (commandName, arguments, stdout) = commandText switch
            {
                "ls /" => ("ls", new[] { "/" }, "workspace.json\n"),
                "cat /workspace.json" => (
                    "cat",
                    new[] { "/workspace.json" },
                    "{\"settings\":{}}\n"),
                "pwd" => ("pwd", Array.Empty<string>(), "/\n"),
                "echo ''" => ("echo", new[] { string.Empty }, "\n"),
                "echo -n reados-native-proxy" => (
                    "echo",
                    new[] { "-n", "reados-native-proxy" },
                    "reados-native-proxy"),
                _ => throw new InvalidOperationException(
                    $"Unexpected native smoke command: {commandText}")
            };

            var isWorkspaceCommand = commandName is "ls" or "cat";
            if (isWorkspaceCommand)
            {
                Assert.Equal(
                    expectedNativeWorkspaceRoot,
                    request.GetProperty("workspaceRoot").GetString());
            }
            else
            {
                Assert.False(request.TryGetProperty("workspaceRoot", out _));
                Assert.Empty(request.GetProperty("environment").EnumerateObject());
            }

            var actor = request.GetProperty("actor").GetString();
            var sessionId = request.GetProperty("sessionId").GetString();
            var workingDirectory = request.GetProperty("workingDirectory").GetString();
            var stdoutBytes = System.Text.Encoding.UTF8.GetBytes(stdout);
            return JsonSerializer.SerializeToUtf8Bytes(new
            {
                contractVersion = MspNativeContract.Version,
                stdout,
                stderr = string.Empty,
                stdoutBytesBase64 = Convert.ToBase64String(stdoutBytes),
                stderrBytesBase64 = string.Empty,
                exitCode = 0,
                auditRecords = new[]
                {
                    new
                    {
                        runId = "package-smoke-native-1",
                        commandLine = commandText,
                        commandName,
                        arguments,
                        exitCode = 0,
                        startedAtUnixMs = 1UL,
                        endedAtUnixMs = 2UL,
                        actor,
                        sessionId,
                        workingDirectory,
                        policyDecision = new
                        {
                            kind = "allow",
                            reason = (string?)null,
                            prompt = (string?)null
                        },
                        diagnostics = Array.Empty<object>()
                    }
                },
                diagnostics = Array.Empty<object>()
            });
        }

        private static byte[] CreateParseResponse(string commandText)
        {
            var (commandName, arguments, argumentWords) = commandText switch
            {
                "pwd" => (
                    "pwd",
                    Array.Empty<string>(),
                    Array.Empty<object>()),
                "echo ''" => (
                    "echo",
                    new[] { string.Empty },
                    new object[] { ExplicitEmptyWord() }),
                "echo -n reados-native-proxy" => (
                    "echo",
                    new[] { "-n", "reados-native-proxy" },
                    new object[] { Word("-n"), Word("reados-native-proxy") }),
                _ => throw new InvalidOperationException(
                    $"Unexpected native parse command: {commandText}")
            };

            return JsonSerializer.SerializeToUtf8Bytes(new
            {
                contractVersion = MspNativeContract.Version,
                succeeded = true,
                script = new
                {
                    rawInput = commandText,
                    pipelines = new[]
                    {
                        new
                        {
                            leadingOperator = (string?)null,
                            isNegated = false,
                            commands = new[]
                            {
                                new
                                {
                                    commandName,
                                    arguments,
                                    assignments = Array.Empty<object>(),
                                    redirections = Array.Empty<object>(),
                                    isAssignmentOnly = false,
                                    rawInput = commandText,
                                    commandNameWord = Word(commandName),
                                    argumentWords
                                }
                            },
                            pipeOperators = Array.Empty<object>()
                        }
                    }
                },
                error = (object?)null
            });
        }

        private static object Word(string text)
        {
            return new
            {
                parts = new[]
                {
                    new
                    {
                        text,
                        isExpandable = true,
                        isQuoted = false
                    }
                },
                hasExplicitEmptyQuotedFragment = false
            };
        }

        private static object ExplicitEmptyWord()
        {
            return new
            {
                parts = new[]
                {
                    new
                    {
                        text = string.Empty,
                        isExpandable = true,
                        isQuoted = true
                    }
                },
                hasExplicitEmptyQuotedFragment = true
            };
        }

        public void Dispose()
        {
        }
    }

    private sealed class PathLeakingNativeAdapter :
        IMspNativeAdapter,
        IMspNativeRuntimeInfoProvider
    {
        private readonly string nativeWorkspaceRoot;

        public PathLeakingNativeAdapter(string nativeWorkspaceRoot)
        {
            this.nativeWorkspaceRoot = nativeWorkspaceRoot;
        }

        public MspNativeRuntimeInfo NativeRuntimeInfo { get; } = new(
            MspNativeAbiMode.LengthDelimitedV2,
            2,
            0,
            0x324D534F44414552UL,
            0x000000000000000FUL);

        public MspNativeCommandResult Execute(MspNativeCommandRequest request)
        {
            throw new InvalidOperationException(
                $"Synthetic native failure at {nativeWorkspaceRoot}.");
        }

        public MspNativeShellParseResult Parse(MspNativeShellParseRequest request)
        {
            throw new NotSupportedException();
        }

        public MspNativeWorkspacePathResult NormalizeWorkspacePath(
            MspNativeWorkspacePathRequest request)
        {
            throw new NotSupportedException();
        }

        public void Dispose()
        {
        }
    }

    private sealed class ReversibleTestDataProtector : ICurrentUserDataProtector
    {
        public byte[] Protect(byte[] plaintext, byte[] entropy)
        {
            return Transform(plaintext, entropy);
        }

        public byte[] Unprotect(byte[] protectedPayload, byte[] entropy)
        {
            return Transform(protectedPayload, entropy);
        }

        private static byte[] Transform(byte[] value, byte[] entropy)
        {
            var result = new byte[value.Length];
            for (var index = 0; index < value.Length; index++)
            {
                result[index] = (byte)(value[index] ^ entropy[index % entropy.Length] ^ 0x5A);
            }

            return result;
        }
    }

    private sealed class ThrowingTestDataProtector : ICurrentUserDataProtector
    {
        public byte[] Protect(byte[] plaintext, byte[] entropy)
        {
            throw new CryptographicException("synthetic credential protection failure");
        }

        public byte[] Unprotect(byte[] protectedPayload, byte[] entropy)
        {
            throw new CryptographicException("synthetic credential protection failure");
        }
    }

    private sealed class TemporaryArtifactsDirectory : IDisposable
    {
        private readonly string root = Path.Combine(
            Path.GetTempPath(),
            "ReadOS.App.Tests",
            Guid.NewGuid().ToString("N"));

        public TemporaryArtifactsDirectory()
        {
            SmokeRoot = Path.Combine(root, "artifacts", "smoke", "package");
            Directory.CreateDirectory(SmokeRoot);
            NativeRunRoot = Path.Combine(
                Path.GetTempPath(),
                ReadOsPackageSmokeOptions.NativeWorkspaceTemporaryDirectoryName,
                "test-" + Guid.NewGuid().ToString("N"));
            NativeWorkspaceRoot = Path.Combine(NativeRunRoot, "workspace");
            Directory.CreateDirectory(NativeWorkspaceRoot);
        }

        public string SmokeRoot { get; }

        public string NativeRunRoot { get; }

        public string NativeWorkspaceRoot { get; }

        public void Dispose()
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
            if (Directory.Exists(NativeRunRoot))
            {
                Directory.Delete(NativeRunRoot, recursive: true);
            }
        }
    }
}

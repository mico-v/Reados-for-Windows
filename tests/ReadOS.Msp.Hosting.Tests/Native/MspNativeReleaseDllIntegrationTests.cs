using ReadOS.Msp.Hosting.Native;
using System.Text.Json;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeReleaseDllIntegrationTests
{
    [ExplicitNativeDllFact]
    public void Explicit_release_dll_enforces_parse_operation_request_cap_without_disclosure()
    {
        var libraryPath = Environment.GetEnvironmentVariable(ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        using var adapter = MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = libraryPath
        });
        const string secret = @"V:\private\oversized-parse-secret";
        var commandText = $"echo {secret} {new string('x', 160 * 1024)}";
        var requestJson = JsonSerializer.SerializeToUtf8Bytes(new
        {
            contractVersion = MspNativeContract.Version,
            commandText
        });
        Assert.True(requestJson.Length > 128 * 1024);
        Assert.True(requestJson.Length < MspNativeAdapterLimits.DefaultMaximumRequestBytes);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Parse(new MspNativeShellParseRequest
            {
                CommandText = commandText
            }));

        Assert.Equal(MspNativeFailureKind.NativeRequestTooLarge, exception.FailureKind);
        Assert.Equal(MspNativeOperation.Parse, exception.Operation);
        Assert.Equal("msp.native.request_too_large", exception.Diagnostic.Code);
        Assert.DoesNotContain(secret, exception.ToString());
        Assert.Null(exception.InnerException);
    }

    [ExplicitNativeDllFact]
    public void Explicit_release_dll_matches_execute_parse_and_path_contracts()
    {
        var libraryPath = Environment.GetEnvironmentVariable(ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        Assert.True(Path.IsPathFullyQualified(libraryPath));
        Assert.True(File.Exists(libraryPath));

        using var adapter = MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = libraryPath
        });

        var runtimeInfo = ((IMspNativeRuntimeInfoProvider)adapter).NativeRuntimeInfo;
        Assert.Equal(MspNativeAbiMode.LengthDelimitedV2, runtimeInfo.AbiMode);
        Assert.Equal(2U, runtimeInfo.MajorVersion);
        Assert.Equal(0U, runtimeInfo.MinorVersion);
        Assert.Equal(MspNativeContract.AbiV2ContractId, runtimeInfo.ContractId);
        Assert.Equal(
            MspNativeContract.AbiV2RequiredCapabilities,
            runtimeInfo.Capabilities & MspNativeContract.AbiV2RequiredCapabilities);

        var pwd = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "pwd",
            WorkingDirectory = "/documents",
            Actor = "dotnet-integration",
            SessionId = "native-release"
        });
        Assert.Equal(0, pwd.ExitCode);
        Assert.Equal("/documents\n", pwd.StdoutText);

        var echo = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "echo ''"
        });
        Assert.Equal("\n", echo.StdoutText);

        var unknown = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "missing-command"
        });
        Assert.Equal(127, unknown.ExitCode);
        Assert.Equal("msp.command_not_found", Assert.Single(unknown.Diagnostics).Code);

        var parsed = adapter.Parse(new MspNativeShellParseRequest
        {
            CommandText = "echo ''"
        });
        Assert.True(parsed.Succeeded);
        Assert.True(Assert.Single(
            Assert.Single(parsed.Script!.Pipelines).Commands[0].ArgumentWords)
            .HasExplicitEmptyQuotedFragment);

        var normalized = adapter.NormalizeWorkspacePath(new MspNativeWorkspacePathRequest
        {
            Path = "../../reports/a.txt",
            CurrentDirectory = "/docs/current"
        });
        Assert.True(normalized.Succeeded);
        Assert.Equal("/reports/a.txt", normalized.VirtualPath);

        var workspaceRoot = Path.Combine(
            Path.GetTempPath(),
            "reados-native-integration-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(workspaceRoot);
        try
        {
            var binary = new byte[] { 0x00, 0xff, (byte)'A', (byte)'\n' };
            File.WriteAllBytes(Path.Combine(workspaceRoot, "binary.dat"), binary);

            var listed = adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "ls /",
                WorkspaceRoot = workspaceRoot
            });
            Assert.Equal(0, listed.ExitCode);
            Assert.Contains("binary.dat\n", listed.StdoutText);

            var read = adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "cat /binary.dat",
                WorkspaceRoot = workspaceRoot
            });
            Assert.Equal(0, read.ExitCode);
            Assert.Equal(binary, read.StdoutBytes.ToArray());
            Assert.DoesNotContain(workspaceRoot, listed.StdoutText);
            Assert.DoesNotContain(workspaceRoot, listed.StderrText);
            Assert.DoesNotContain(workspaceRoot, JsonSerializer.Serialize(listed.Diagnostics));
            Assert.DoesNotContain(workspaceRoot, JsonSerializer.Serialize(listed.AuditRecords));
        }
        finally
        {
            Directory.Delete(workspaceRoot, recursive: true);
        }
    }

    [ExplicitNativeDllFact]
    public async Task Explicit_release_dll_runs_through_managed_policy_and_single_audit_proxy()
    {
        var libraryPath = Environment.GetEnvironmentVariable(
            ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        using var provider = new LazyMspNativeAdapterProvider(() =>
            MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
            {
                LibraryPath = libraryPath
            }));
        var registry = MspRuntime.CreateDefaultRegistry();
        Assert.True(registry.TryGet("pwd", out var pwdDefinition));
        Assert.True(registry.TryGet("echo", out var echoDefinition));
        registry.Register(new MspNativeBackedCommand(pwdDefinition, provider));
        registry.Register(new MspNativeBackedCommand(echoDefinition, provider));
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
        var host = new MspRuntimeCommandHost(new MspRuntime(context));

        var pwd = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "pwd",
            WorkingDirectory = "/documents",
            Actor = "dotnet-proxy",
            SessionId = "native-proxy"
        });
        var echo = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "echo ''",
            Actor = "dotnet-proxy",
            SessionId = "native-proxy"
        });

        Assert.Equal("/documents\n", pwd.Stdout);
        Assert.Equal("\n", echo.Stdout);
        Assert.Single(pwd.AuditRecords);
        Assert.Single(echo.AuditRecords);
        Assert.Equal(MspPolicyDecision.Allow, pwd.AuditRecords[0].Decision);
        Assert.Equal(MspPolicyDecision.Allow, echo.AuditRecords[0].Decision);
        Assert.Equal("pwd", pwd.AuditRecords[0].CommandName);
        Assert.Equal("echo", echo.AuditRecords[0].CommandName);
    }

    [ExplicitNativeDllFact]
    public void Explicit_release_dll_serves_in_memory_workspace_reads_differentially()
    {
        var libraryPath = Environment.GetEnvironmentVariable(
            ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        using var adapter = MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = libraryPath
        });

        var baseWorkspace = new InMemoryNativeWorkspace();
        var mediaWorkspace = new InMemoryNativeWorkspace();
        var binary = new byte[] { 0x00, 0xff, (byte)'A', (byte)'\n' };
        baseWorkspace.AddFile("/base/readme.txt", "hello base\n"u8.ToArray());
        baseWorkspace.AddDirectory("/base");
        // The mount backend's namespace is rooted at the mount point: a file at
        // virtual /media/data.bin is served from backend /data.bin.
        mediaWorkspace.AddFile("/data.bin", binary);

        var invocation = new MspNativeWorkspaceInvocation(
            callbackBase: new MspNativeWorkspaceBackend
            {
                Id = 1,
                Workspace = baseWorkspace
            },
            mounts:
            [
                new MspNativeWorkspaceMount
                {
                    Path = "/media",
                    Backend = new MspNativeWorkspaceBackend
                    {
                        Id = 2,
                        Workspace = mediaWorkspace
                    }
                }
            ]);

        var stat = adapter.ReadWorkspace(
            invocation,
            MspNativeWorkspaceReadOperation.Stat,
            "/base/readme.txt");
        var statInfo = Assert.IsType<MspNativeWorkspaceReadResult.FileInfoResult>(stat).FileInfo;
        Assert.Equal(MspNativeWorkspaceFileType.RegularFile, statInfo.FileType);
        Assert.Equal((ulong)"hello base\n".Length, statInfo.SizeBytes);
        Assert.Equal("volume:fixture", statInfo.FileIdentity);

        var directoryStat = adapter.ReadWorkspace(
            invocation,
            MspNativeWorkspaceReadOperation.Stat,
            "/media");
        Assert.Equal(
            MspNativeWorkspaceFileType.Directory,
            Assert.IsType<MspNativeWorkspaceReadResult.FileInfoResult>(directoryStat)
                .FileInfo.FileType);

        var list = adapter.ReadWorkspace(
            invocation,
            MspNativeWorkspaceReadOperation.ListDirectory,
            "/media");
        var entries = Assert.IsType<MspNativeWorkspaceReadResult.EntriesResult>(list).Entries;
        var entry = Assert.Single(entries);
        Assert.Equal("data.bin", entry.Name);
        Assert.Equal(MspNativeWorkspaceFileType.RegularFile, entry.Info.FileType);
        Assert.Equal((ulong)binary.Length, entry.Info.SizeBytes);

        var read = adapter.ReadWorkspace(
            invocation,
            MspNativeWorkspaceReadOperation.ReadFileRange,
            "/media/data.bin",
            offset: 1,
            length: 3);
        Assert.Equal(
            new byte[] { 0xff, (byte)'A', (byte)'\n' },
            Assert.IsType<MspNativeWorkspaceReadResult.BytesResult>(read).Bytes.ToArray());

        var missing = Assert.Throws<MspNativeWorkspaceException>(() =>
            adapter.ReadWorkspace(
                invocation,
                MspNativeWorkspaceReadOperation.Stat,
                "/media/missing.txt"));
        Assert.Equal(MspNativeWorkspaceErrorKind.NotFound, missing.ErrorKind);
    }

    [ExplicitNativeDllFact]
    public void Explicit_release_dll_execs_an_echo_command_session()
    {
        var libraryPath = Environment.GetEnvironmentVariable(
            ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        using var adapter = MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = libraryPath
        });

        var result = adapter.ExecSession(new MspNativeExecSessionRequest
        {
            CommandText = "echo hello-session",
            WorkingDirectory = "/",
            Actor = "dotnet-integration",
            YieldTimeMs = 1000,
            MaxOutputTokens = 1024
        });

        Assert.True(result.Ok);
        Assert.True(result.SessionId > 0);
        Assert.False(result.Running);
        Assert.Equal(0, result.ExitCode);
        Assert.Contains("hello-session", result.TerminalText);
        Assert.Null(result.Error);
    }

    private sealed class InMemoryNativeWorkspace : IMspNativeReadOnlyWorkspace
    {
        private readonly Dictionary<string, MspNativeWorkspaceFileInfo> files =
            new(StringComparer.Ordinal);
        private readonly HashSet<string> directories = new(StringComparer.Ordinal);
        private readonly Dictionary<string, byte[]> contents = new(StringComparer.Ordinal);

        public void AddFile(string virtualPath, byte[] bytes)
        {
            files[virtualPath] = new MspNativeWorkspaceFileInfo
            {
                FileType = MspNativeWorkspaceFileType.RegularFile,
                SizeBytes = checked((ulong)bytes.Length),
                ModificationTimeUnixMs = 1_700_000_000_000L,
                FileIdentity = "volume:fixture"
            };
            contents[virtualPath] = bytes;
        }

        public void AddDirectory(string virtualPath)
        {
            directories.Add(virtualPath);
        }

        public ValueTask<MspNativeWorkspaceFileInfo> StatAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            if (virtualPath == "/")
            {
                return ValueTask.FromResult(new MspNativeWorkspaceFileInfo
                {
                    FileType = MspNativeWorkspaceFileType.Directory
                });
            }

            if (files.TryGetValue(virtualPath, out var info))
            {
                return ValueTask.FromResult(info);
            }

            if (directories.Contains(virtualPath))
            {
                return ValueTask.FromResult(new MspNativeWorkspaceFileInfo
                {
                    FileType = MspNativeWorkspaceFileType.Directory
                });
            }

            return ValueTask.FromException<MspNativeWorkspaceFileInfo>(
                new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.NotFound));
        }

        public ValueTask<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>> ListDirectoryAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            if (virtualPath != "/" && !directories.Contains(virtualPath))
            {
                return ValueTask.FromException<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(
                    new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.NotFound));
            }

            var prefixLength = virtualPath == "/" ? 1 : virtualPath.Length + 1;
            var entries = files.Keys
                .Where(path => Parent(path) == virtualPath)
                .Select(path => new MspNativeWorkspaceDirectoryEntry
                {
                    Name = path[prefixLength..],
                    Info = files[path]
                })
                .ToArray();
            return ValueTask.FromResult<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(entries);
        }

        public ValueTask<ReadOnlyMemory<byte>> ReadFileRangeAsync(
            string virtualPath,
            ulong offset,
            int length,
            CancellationToken cancellationToken = default)
        {
            if (!contents.TryGetValue(virtualPath, out var bytes))
            {
                return ValueTask.FromException<ReadOnlyMemory<byte>>(
                    new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.NotFound));
            }

            if (offset > (ulong)bytes.Length)
            {
                return ValueTask.FromException<ReadOnlyMemory<byte>>(
                    new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.InvalidPath));
            }

            var start = checked((int)offset);
            var count = Math.Min(length, bytes.Length - start);
            return ValueTask.FromResult<ReadOnlyMemory<byte>>(
                new ReadOnlyMemory<byte>(bytes, start, count));
        }

        private static string Parent(string virtualPath)
        {
            var separator = virtualPath.LastIndexOf('/');
            return separator <= 0 ? "/" : virtualPath[..separator];
        }
    }
}

internal sealed class ExplicitNativeDllFactAttribute : FactAttribute
{
    public const string EnvironmentVariable = "READOS_MSP_NATIVE_DLL";

    public ExplicitNativeDllFactAttribute()
    {
        if (!OperatingSystem.IsWindows())
        {
            Skip = "The real native MSP transport is Windows-only.";
            return;
        }

        if (string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable(EnvironmentVariable)))
        {
            Skip = $"Set {EnvironmentVariable} to an explicit built msp_core.dll path.";
        }
    }
}

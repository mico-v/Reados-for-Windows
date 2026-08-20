using System.Text;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeReadOnlyCommandRoutingTests
{
    [Fact]
    public async Task Canonical_ls_matches_managed_virtual_listing_and_records_one_audit()
    {
        var workspace = new DifferentialWorkspace();
        workspace.AddDirectory("/docs");
        workspace.AddFile("/readme.txt", "hello\n"u8.ToArray());
        using var adapter = new CallbackRoutingAdapter();
        using var provider = new LazyMspNativeAdapterProvider(() => adapter);
        var host = CreateHost(workspace, provider);

        var managed = await new LsCommand().ExecuteAsync(
            new MspCommandContext(
                workspace,
                MspRuntime.CreateDefaultRegistry(),
                new AllowAllMspPolicy(),
                new InMemoryMspAuditSink()),
            ["/"]);
        var native = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "ls /",
            Actor = "tester",
            SessionId = "read-only-list"
        });

        Assert.True(native.Succeeded, native.Stderr);
        Assert.Equal(managed.Stdout, native.Stdout);
        Assert.Equal(1, adapter.ReadWorkspaceCalls);
        Assert.Equal(MspNativeWorkspaceReadOperation.ListDirectory, adapter.ReadRequests[0].Operation);
        Assert.Equal("/", adapter.ReadRequests[0].VirtualPath);
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Single(native.AuditRecords);
        Assert.DoesNotContain("V:\\ReadOS-Test", native.Stdout + native.Stderr, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Canonical_text_cat_matches_managed_virtual_read_and_records_one_audit()
    {
        var workspace = new DifferentialWorkspace();
        workspace.AddFile("/docs/readme.txt", "hello\n"u8.ToArray());
        using var adapter = new CallbackRoutingAdapter();
        using var provider = new LazyMspNativeAdapterProvider(() => adapter);
        var host = CreateHost(workspace, provider);

        var managed = await new CatCommand().ExecuteAsync(
            new MspCommandContext(
                workspace,
                MspRuntime.CreateDefaultRegistry(),
                new AllowAllMspPolicy(),
                new InMemoryMspAuditSink()),
            ["/docs/readme.txt"]);
        var native = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "cat /docs/readme.txt",
            Actor = "tester",
            SessionId = "read-only-cat"
        });

        Assert.True(native.Succeeded, native.Stderr);
        Assert.Equal(managed.Stdout, native.Stdout);
        Assert.Equal(
            [MspNativeWorkspaceReadOperation.Stat, MspNativeWorkspaceReadOperation.ReadFileRange],
            adapter.ReadRequests.Select(request => request.Operation));
        Assert.Equal(0, adapter.ExecuteCalls);
        Assert.Single(native.AuditRecords);
    }

    [Fact]
    public async Task Binary_cat_fails_closed_without_disclosing_a_host_path()
    {
        var workspace = new DifferentialWorkspace();
        workspace.AddFile("/artifacts/data.bin", [0x00, 0xff, 0x41]);
        using var adapter = new CallbackRoutingAdapter();
        using var provider = new LazyMspNativeAdapterProvider(() => adapter);
        var host = CreateHost(workspace, provider);

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "cat /artifacts/data.bin",
            Actor = "tester",
            SessionId = "binary-cat"
        });

        Assert.False(result.Succeeded);
        Assert.Equal("msp.native.binary_output_not_supported", Assert.Single(result.Diagnostics).Code);
        Assert.Empty(result.Stdout);
        Assert.DoesNotContain("V:\\ReadOS-Test", result.Stderr, StringComparison.Ordinal);
        Assert.Single(result.AuditRecords);
    }

    [Theory]
    [InlineData("ls /missing", "No such file or directory: /missing")]
    [InlineData("cat /missing", "Cannot read text file: /missing")]
    [InlineData("cat /.msp/secret", "Cannot read text file: /")]
    public async Task Missing_and_hidden_virtual_paths_fail_closed_with_one_audit(
        string commandText,
        string expectedError)
    {
        var workspace = new DifferentialWorkspace();
        using var adapter = new CallbackRoutingAdapter();
        using var provider = new LazyMspNativeAdapterProvider(() => adapter);
        var host = CreateHost(workspace, provider);

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = commandText,
            Actor = "tester",
            SessionId = "hidden-missing"
        });

        Assert.False(result.Succeeded);
        Assert.Contains(expectedError, result.Stderr, StringComparison.Ordinal);
        Assert.Single(result.AuditRecords);
        Assert.DoesNotContain("V:\\ReadOS-Test", result.Stderr, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Unsupported_ls_operand_shape_fails_before_workspace_read()
    {
        var workspace = new DifferentialWorkspace();
        using var adapter = new CallbackRoutingAdapter();
        using var provider = new LazyMspNativeAdapterProvider(() => adapter);
        var host = CreateHost(workspace, provider);

        var result = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "ls /one /two",
            Actor = "tester",
            SessionId = "unsupported-ls"
        });

        Assert.False(result.Succeeded);
        Assert.Equal("msp.native.route.unsupported_arguments", Assert.Single(result.Diagnostics).Code);
        Assert.Empty(adapter.ReadRequests);
        Assert.Single(result.AuditRecords);
    }

    [Fact]
    public async Task Physical_input_and_malformed_native_entry_never_reach_results()
    {
        var workspace = new DifferentialWorkspace();
        workspace.AddDirectory("/");
        using var adapter = new CallbackRoutingAdapter
        {
            ReadResultFactory = (_, operation, _, _, _) => operation == MspNativeWorkspaceReadOperation.ListDirectory
                ? MspNativeWorkspaceReadResult.Entries(
                [
                    new MspNativeWorkspaceDirectoryEntry
                    {
                        Name = @"V:\\private\\secret.txt",
                        Info = new MspNativeWorkspaceFileInfo
                        {
                            FileType = MspNativeWorkspaceFileType.RegularFile
                        }
                    }
                ])
                : throw new InvalidOperationException("unexpected operation")
        };
        using var provider = new LazyMspNativeAdapterProvider(() => adapter);
        var host = CreateHost(workspace, provider);

        var physicalInput = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = @"cat V:\\private\\secret.txt",
            Actor = "tester",
            SessionId = "physical-input"
        });
        var physicalReadCount = adapter.ReadRequests.Count;
        var malformedResult = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "ls /",
            Actor = "tester",
            SessionId = "malformed-result"
        });

        Assert.False(physicalInput.Succeeded);
        Assert.DoesNotContain(@"V:\\private\\secret.txt", physicalInput.Stderr, StringComparison.Ordinal);
        Assert.Equal(0, physicalReadCount);
        Assert.False(malformedResult.Succeeded);
        Assert.Equal("msp.native.workspace_result_mismatch", Assert.Single(malformedResult.Diagnostics).Code);
        Assert.DoesNotContain(@"V:\\private\\secret.txt", malformedResult.Stderr, StringComparison.Ordinal);
        Assert.Single(physicalInput.AuditRecords);
        Assert.Single(malformedResult.AuditRecords);
    }

    private static MspRuntimeCommandHost CreateHost(
        DifferentialWorkspace workspace,
        IMspNativeAdapterProvider provider)
    {
        var registry = MspRuntime.CreateDefaultRegistry();
        Assert.True(registry.TryGet("ls", out var ls));
        Assert.True(registry.TryGet("cat", out var cat));
        registry.Register(new MspNativeBackedCommand(ls, provider));
        registry.Register(new MspNativeBackedCommand(cat, provider));
        return new MspRuntimeCommandHost(
            new MspRuntime(
                new MspCommandContext(
                    workspace,
                    registry,
                    new AllowAllMspPolicy(),
                    new InMemoryMspAuditSink())));
    }

    private sealed class CallbackRoutingAdapter : IMspNativeAdapter
    {
        public Func<
            MspNativeWorkspaceInvocation,
            MspNativeWorkspaceReadOperation,
            string,
            ulong,
            int,
            MspNativeWorkspaceReadResult>? ReadResultFactory { get; init; }

        public List<ReadRequest> ReadRequests { get; } = [];

        public int ReadWorkspaceCalls { get; private set; }

        public int ExecuteCalls { get; private set; }

        public MspNativeShellParseResult Parse(MspNativeShellParseRequest request)
        {
            var tokens = request.CommandText.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            var commandName = tokens[0];
            var arguments = tokens.Skip(1).ToArray();
            return new MspNativeShellParseResult
            {
                ContractVersion = MspNativeContract.Version,
                Succeeded = true,
                Script = new MspNativeParsedShellScript
                {
                    RawInput = request.CommandText,
                    Pipelines =
                    [
                        new MspNativeParsedCommandPipeline
                        {
                            LeadingOperator = null,
                            IsNegated = false,
                            Commands =
                            [
                                new MspNativeParsedCommandLine
                                {
                                    CommandName = commandName,
                                    Arguments = arguments,
                                    Assignments = [],
                                    Redirections = [],
                                    IsAssignmentOnly = false,
                                    RawInput = request.CommandText,
                                    CommandNameWord = Word(commandName),
                                    ArgumentWords = arguments.Select(Word).ToArray()
                                }
                            ],
                            PipeOperators = []
                        }
                    ]
                }
            };
        }

        public MspNativeWorkspaceReadResult ReadWorkspace(
            MspNativeWorkspaceInvocation invocation,
            MspNativeWorkspaceReadOperation operation,
            string virtualPath,
            ulong offset = 0,
            int length = 0,
            CancellationToken cancellationToken = default)
        {
            ReadWorkspaceCalls++;
            ReadRequests.Add(new ReadRequest(operation, virtualPath, offset, length));
            return ReadResultFactory?.Invoke(invocation, operation, virtualPath, offset, length)
                ?? ReadThroughCallbacks(invocation, operation, virtualPath, offset, length, cancellationToken);
        }

        public MspNativeCommandResult Execute(MspNativeCommandRequest request)
        {
            ExecuteCalls++;
            throw new InvalidOperationException("read-only commands must not use Execute");
        }

        public MspNativeWorkspacePathResult NormalizeWorkspacePath(MspNativeWorkspacePathRequest request)
        {
            throw new NotSupportedException();
        }

        public void Dispose()
        {
        }

        private static MspNativeWorkspaceReadResult ReadThroughCallbacks(
            MspNativeWorkspaceInvocation invocation,
            MspNativeWorkspaceReadOperation operation,
            string virtualPath,
            ulong offset,
            int length,
            CancellationToken cancellationToken)
        {
            var workspace = invocation.CallbackBase!.Workspace;
            return operation switch
            {
                MspNativeWorkspaceReadOperation.Stat => MspNativeWorkspaceReadResult.FileInfo(
                    workspace.StatAsync(virtualPath, cancellationToken).GetAwaiter().GetResult()),
                MspNativeWorkspaceReadOperation.ListDirectory => MspNativeWorkspaceReadResult.Entries(
                    workspace.ListDirectoryAsync(virtualPath, cancellationToken).GetAwaiter().GetResult()),
                MspNativeWorkspaceReadOperation.ReadFileRange => MspNativeWorkspaceReadResult.Bytes(
                    workspace.ReadFileRangeAsync(virtualPath, offset, length, cancellationToken).GetAwaiter().GetResult()),
                _ => throw new ArgumentOutOfRangeException(nameof(operation))
            };
        }

        private static MspNativeParsedWord Word(string text)
        {
            return new MspNativeParsedWord
            {
                Parts =
                [
                    new MspNativeParsedWordPart
                    {
                        Text = text,
                        IsExpandable = true,
                        IsQuoted = false
                    }
                ],
                HasExplicitEmptyQuotedFragment = false
            };
        }
    }

    private sealed record ReadRequest(
        MspNativeWorkspaceReadOperation Operation,
        string VirtualPath,
        ulong Offset,
        int Length);

    private sealed class DifferentialWorkspace : IMspWorkspace, IMspNativeReadOnlyWorkspace
    {
        private static readonly Encoding StrictUtf8 = new UTF8Encoding(false, true);
        private readonly Dictionary<string, byte[]> files = new(StringComparer.Ordinal);
        private readonly HashSet<string> directories = new(StringComparer.Ordinal) { "/" };

        public string NormalizePath(string path, string workingDirectory = "/")
        {
            return MspPathUtility.Normalize(path, workingDirectory);
        }

        public ValueTask<bool> ExistsAsync(string path, CancellationToken cancellationToken = default)
        {
            var normalized = NormalizePath(path);
            return ValueTask.FromResult(files.ContainsKey(normalized) || directories.Contains(normalized));
        }

        public ValueTask<IReadOnlyList<MspWorkspaceEntry>> ListAsync(
            string path,
            CancellationToken cancellationToken = default)
        {
            var normalized = NormalizePath(path);
            var entries = files
                .Where(item => Parent(item.Key) == normalized)
                .Select(item => new MspWorkspaceEntry
                {
                    Path = item.Key,
                    Name = Name(item.Key),
                    IsDirectory = false,
                    SizeBytes = item.Value.LongLength
                })
                .Concat(directories
                    .Where(item => item != normalized && Parent(item) == normalized)
                    .Select(item => new MspWorkspaceEntry
                    {
                        Path = item,
                        Name = Name(item),
                        IsDirectory = true
                    }))
                .ToArray();
            return ValueTask.FromResult<IReadOnlyList<MspWorkspaceEntry>>(entries);
        }

        public ValueTask<string?> TryReadTextAsync(
            string path,
            CancellationToken cancellationToken = default)
        {
            var normalized = NormalizePath(path);
            if (!files.TryGetValue(normalized, out var bytes))
            {
                return ValueTask.FromResult<string?>(null);
            }

            return ValueTask.FromResult<string?>(Encoding.UTF8.GetString(bytes));
        }

        public ValueTask WriteTextAsync(
            string path,
            string content,
            CancellationToken cancellationToken = default)
        {
            AddFile(path, StrictUtf8.GetBytes(content));
            return ValueTask.CompletedTask;
        }

        public ValueTask<bool> TryDeleteAsync(
            string path,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(files.Remove(NormalizePath(path)));
        }

        public ValueTask<MspNativeWorkspaceFileInfo> StatAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            EnsureVisible(virtualPath);
            if (files.TryGetValue(virtualPath, out var bytes))
            {
                return ValueTask.FromResult(new MspNativeWorkspaceFileInfo
                {
                    FileType = MspNativeWorkspaceFileType.RegularFile,
                    SizeBytes = checked((ulong)bytes.Length)
                });
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
            EnsureVisible(virtualPath);
            if (!directories.Contains(virtualPath))
            {
                return ValueTask.FromException<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(
                    new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.NotFound));
            }

            var entries = files
                .Where(item => Parent(item.Key) == virtualPath)
                .Select(item => new MspNativeWorkspaceDirectoryEntry
                {
                    Name = Name(item.Key),
                    Info = new MspNativeWorkspaceFileInfo
                    {
                        FileType = MspNativeWorkspaceFileType.RegularFile,
                        SizeBytes = checked((ulong)item.Value.Length)
                    }
                })
                .Concat(directories
                    .Where(item => item != virtualPath && Parent(item) == virtualPath)
                    .Select(item => new MspNativeWorkspaceDirectoryEntry
                    {
                        Name = Name(item),
                        Info = new MspNativeWorkspaceFileInfo
                        {
                            FileType = MspNativeWorkspaceFileType.Directory
                        }
                    }))
                .ToArray();
            return ValueTask.FromResult<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(entries);
        }

        public ValueTask<ReadOnlyMemory<byte>> ReadFileRangeAsync(
            string virtualPath,
            ulong offset,
            int length,
            CancellationToken cancellationToken = default)
        {
            EnsureVisible(virtualPath);
            if (!files.TryGetValue(virtualPath, out var bytes))
            {
                return ValueTask.FromException<ReadOnlyMemory<byte>>(
                    new MspNativeWorkspaceException(
                        directories.Contains(virtualPath)
                            ? MspNativeWorkspaceErrorKind.IsDirectory
                            : MspNativeWorkspaceErrorKind.NotFound));
            }

            if (offset > (ulong)bytes.Length)
            {
                return ValueTask.FromException<ReadOnlyMemory<byte>>(
                    new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.InvalidPath));
            }

            var start = checked((int)offset);
            return ValueTask.FromResult<ReadOnlyMemory<byte>>(
                bytes.AsMemory(start, Math.Min(length, bytes.Length - start)));
        }

        public void AddDirectory(string path)
        {
            var normalized = NormalizePath(path);
            var current = "/";
            foreach (var component in normalized.Split('/', StringSplitOptions.RemoveEmptyEntries))
            {
                current = current == "/" ? "/" + component : current + "/" + component;
                directories.Add(current);
            }
        }

        public void AddFile(string path, byte[] bytes)
        {
            var normalized = NormalizePath(path);
            AddDirectory(Parent(normalized));
            files[normalized] = bytes.ToArray();
        }

        private static void EnsureVisible(string path)
        {
            if (path.Split('/', StringSplitOptions.RemoveEmptyEntries)
                .Any(component => string.Equals(component, ".msp", StringComparison.OrdinalIgnoreCase)))
            {
                throw new MspNativeWorkspaceException(MspNativeWorkspaceErrorKind.HiddenPath);
            }
        }

        private static string Parent(string path)
        {
            var separator = path.LastIndexOf('/');
            return separator <= 0 ? "/" : path[..separator];
        }

        private static string Name(string path)
        {
            return path[(path.LastIndexOf('/') + 1)..];
        }
    }
}

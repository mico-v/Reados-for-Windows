using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Native.RuntimeFfi;

public sealed partial class MspCommandRuntimeFfiAdapterTests
{
    [Fact]
    public void Contract_uses_exact_exports_and_validates_abi_metadata()
    {
        Assert.Equal(
        [
            "msp_command_runtime_ffi_abi_version",
            "msp_command_runtime_ffi_version_data",
            "msp_command_runtime_ffi_runtime_create",
            "msp_command_runtime_ffi_runtime_free",
            "msp_command_runtime_ffi_workspace_create",
            "msp_command_runtime_ffi_workspace_free",
            "msp_command_runtime_ffi_workspace_put_file",
            "msp_command_runtime_ffi_execute_json",
            "msp_command_runtime_ffi_result_exit_code",
            "msp_command_runtime_ffi_result_stdout_data",
            "msp_command_runtime_ffi_result_stderr_data",
            "msp_command_runtime_ffi_result_diagnostic_data",
            "msp_command_runtime_ffi_result_free"
        ], MspCommandRuntimeFfiExports.All);

        using var state = new FakeNativeState();
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);

        Assert.Equal(1U, library.AbiVersion);
        Assert.Equal("0.1.0", library.HeaderVersion);
    }

    [Fact]
    public void Execute_copies_binary_buffers_and_keeps_module_loaded_until_result_release()
    {
        using var state = new FakeNativeState
        {
            Stdout = [0, 255, 1],
            Stderr = [255, 0],
            Diagnostic = Encoding.UTF8.GetBytes("{\"code\":\"msp.ok\"}")
        };
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        using var runtime = library.CreateRuntime();
        using var workspace = library.CreateWorkspace();

        workspace.PutFile("/workspace/input", [0, 255]);
        using var result = library.Execute(
            runtime,
            workspace,
            new MspCommandRuntimeFfiRequest
            {
                Command = "cat",
                Cwd = "/workspace",
                StdinBytes = [0, 255]
            });

        Assert.Equal([0, 255, 1], result.StdoutBytes);
        Assert.Equal([255, 0], result.StderrBytes);
        Assert.Equal("msp.ok", result.DiagnosticCode);
        Assert.Contains("stdinBase64", Encoding.UTF8.GetString(state.LastRequest));

        library.Dispose();
        Assert.False(state.Library.IsDisposed);
        result.Dispose();
        Assert.False(state.Library.IsDisposed);
        runtime.Dispose();
        workspace.Dispose();
        Assert.True(state.Library.IsDisposed);
    }

    [Fact]
    public void Execute_rejects_raw_nul_invalid_utf8_and_oversized_requests_before_native_call()
    {
        using var state = new FakeNativeState();
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        using var runtime = library.CreateRuntime();
        using var workspace = library.CreateWorkspace();

        var rawNulException = Assert.Throws<MspCommandRuntimeFfiException>(() =>
            library.ExecuteJson(runtime, workspace, Encoding.UTF8.GetBytes("{}\0")));
        Assert.Equal(MspCommandRuntimeFfiFailureKind.InvalidArgument, rawNulException.FailureKind);

        var utf8Exception = Assert.Throws<MspCommandRuntimeFfiException>(() =>
            library.ExecuteJson(runtime, workspace, [0xff]));
        Assert.Equal(MspCommandRuntimeFfiFailureKind.InvalidArgument, utf8Exception.FailureKind);

        var escapedNulException = Assert.Throws<MspCommandRuntimeFfiException>(() =>
            library.ExecuteJson(
                runtime,
                workspace,
                Encoding.UTF8.GetBytes("{\"version\":1,\"command\":\"echo \\u0000\",\"cwd\":\"/workspace\"}")));
        Assert.Equal(MspCommandRuntimeFfiFailureKind.InvalidArgument, escapedNulException.FailureKind);

        using var limitedLibrary = MspCommandRuntimeFfiLibrary.CreateForTests(
            state.Library,
            state.Exports,
            new MspCommandRuntimeFfiLimits { MaximumJsonRequestBytes = 4 });
        using var limitedRuntime = limitedLibrary.CreateRuntime();
        using var limitedWorkspace = limitedLibrary.CreateWorkspace();
        var limitException = Assert.Throws<MspCommandRuntimeFfiException>(() =>
            limitedLibrary.ExecuteJson(limitedRuntime, limitedWorkspace, [1, 2, 3, 4, 5]));
        Assert.Equal(MspCommandRuntimeFfiFailureKind.LimitExceeded, limitException.FailureKind);
        Assert.Equal(0, state.ExecuteCalls);
    }

    [Fact]
    public void Execute_rejects_non_virtual_paths_and_undeclared_payload_fields_before_native_call()
    {
        using var state = new FakeNativeState();
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        using var runtime = library.CreateRuntime();
        using var workspace = library.CreateWorkspace();

        var requests = new[]
        {
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"C:\\\\workspace\"}",
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"\\\\server\\\\share\"}",
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/\"}",
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/workspace/../outside\"}",
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/workspace\",\"providerSecret\":\"must-not-cross\"}",
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/workspace\",\"environment\":{\"PATH\":\"must-not-cross\"}}"
        };

        foreach (var request in requests)
        {
            var exception = Assert.Throws<MspCommandRuntimeFfiException>(() =>
                library.ExecuteJson(runtime, workspace, Encoding.UTF8.GetBytes(request)));
            Assert.Equal(MspCommandRuntimeFfiFailureKind.InvalidArgument, exception.FailureKind);
        }

        var oversizedPath = string.Concat(
            "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/",
            new string('x', MspCommandRuntimeFfiLimits.DefaultMaximumVirtualPathBytes),
            "\"}");
        var oversizedPathException = Assert.Throws<MspCommandRuntimeFfiException>(() =>
            library.ExecuteJson(runtime, workspace, Encoding.UTF8.GetBytes(oversizedPath)));
        Assert.Equal(MspCommandRuntimeFfiFailureKind.LimitExceeded, oversizedPathException.FailureKind);
        Assert.Equal(0, state.ExecuteCalls);
    }

    [Fact]
    public void Different_library_handles_are_rejected()
    {
        using var second = new FakeNativeState();
        using var first = new FakeNativeState();
        using var firstLibrary = MspCommandRuntimeFfiLibrary.CreateForTests(first.Library, first.Exports);
        using var secondLibrary = MspCommandRuntimeFfiLibrary.CreateForTests(second.Library, second.Exports);
        using var runtime = firstLibrary.CreateRuntime();
        using var workspace = secondLibrary.CreateWorkspace();

        var exception = Assert.Throws<MspCommandRuntimeFfiException>(() =>
            firstLibrary.ExecuteJson(runtime, workspace, Encoding.UTF8.GetBytes("{}")));
        Assert.Equal(MspCommandRuntimeFfiFailureKind.CrossLibraryHandle, exception.FailureKind);
    }

    [ExplicitCommandRuntimeFfiFact]
    public void Real_dll_test_uses_only_explicit_command_runtime_variable()
    {
        var path = Environment.GetEnvironmentVariable(ExplicitCommandRuntimeFfiFactAttribute.EnvironmentVariable)!;
        Assert.True(Path.IsPathFullyQualified(path));
        Assert.True(File.Exists(path));
        using var library = MspCommandRuntimeFfiLibrary.Load(path);
        Assert.Equal(1U, library.AbiVersion);
        Assert.Equal("0.1.0", library.HeaderVersion);
    }

    private sealed class FakeNativeState : IDisposable
    {
        private readonly ConcurrentDictionary<nint, byte> handles = new();
        private readonly ConcurrentDictionary<nint, (byte[] Stdout, byte[] Stderr, byte[] Diagnostic)> resultBuffers = new();
        private readonly List<Delegate> delegates = [];
        private readonly nint versionPointer;
        private int disposed;

        internal FakeNativeState()
        {
            versionPointer = Marshal.AllocHGlobal(5);
            Marshal.Copy(Encoding.ASCII.GetBytes("0.1.0"), 0, versionPointer, 5);

            MspCommandRuntimeFfiAbiVersionDelegate abiVersion = () => 1;
            MspCommandRuntimeFfiVersionDataDelegate versionData = (out nuint length) =>
            {
                length = 5;
                return versionPointer;
            };
            MspCommandRuntimeFfiRuntimeCreateDelegate runtimeCreate = () => AllocateHandle();
            MspCommandRuntimeFfiRuntimeFreeDelegate runtimeFree = handle => FreeHandle(handle);
            MspCommandRuntimeFfiWorkspaceCreateDelegate workspaceCreate = () => AllocateHandle();
            MspCommandRuntimeFfiWorkspaceFreeDelegate workspaceFree = handle => FreeHandle(handle);
            MspCommandRuntimeFfiWorkspacePutFileDelegate putFile =
                (nint workspace, nint path, nuint pathLength, nint data, nuint dataLength) =>
                {
                    if (!handles.ContainsKey(workspace) || path == nint.Zero || pathLength == 0)
                    {
                        return 1;
                    }

                    LastPath = Copy(path, pathLength);
                    LastFile = Copy(data, dataLength);
                    return 0;
                };
            MspCommandRuntimeFfiExecuteJsonDelegate executeJson =
                (nint runtime, nint workspace, nint request, nuint requestLength) =>
                {
                    if (!handles.ContainsKey(runtime) || !handles.ContainsKey(workspace))
                    {
                        return nint.Zero;
                    }

                    LastRequest = Copy(request, requestLength);
                    Interlocked.Increment(ref ExecuteCalls);
                    var result = AllocateHandle();
                    resultBuffers[result] = (Stdout, Stderr, Diagnostic);
                    return result;
                };
            MspCommandRuntimeFfiResultExitCodeDelegate resultExitCode = _ => ExitCode;
            MspCommandRuntimeFfiResultDataDelegate stdoutData =
                (nint result, out nuint length) => GetResultData(result, 0, out length);
            MspCommandRuntimeFfiResultDataDelegate stderrData =
                (nint result, out nuint length) => GetResultData(result, 1, out length);
            MspCommandRuntimeFfiResultDataDelegate diagnosticData =
                (nint result, out nuint length) => GetResultData(result, 2, out length);
            MspCommandRuntimeFfiResultFreeDelegate resultFree = result =>
            {
                resultBuffers.TryRemove(result, out _);
                FreeHandle(result);
            };

            delegates.AddRange([
                abiVersion, versionData, runtimeCreate, runtimeFree,
                workspaceCreate, workspaceFree, putFile, executeJson,
                resultExitCode, stdoutData, stderrData, diagnosticData, resultFree]);
            Exports = new MspCommandRuntimeFfiNativeExports(
                abiVersion, versionData, runtimeCreate, runtimeFree,
                workspaceCreate, workspaceFree, putFile, executeJson,
                resultExitCode, stdoutData, stderrData, diagnosticData, resultFree);
            Library = new FakeNativeLibrary();
        }

        internal FakeNativeLibrary Library { get; }
        internal MspCommandRuntimeFfiNativeExports Exports { get; }
        internal byte[] Stdout { get; set; } = [1];
        internal byte[] Stderr { get; set; } = [2];
        internal byte[] Diagnostic { get; set; } = Encoding.UTF8.GetBytes("{\"code\":\"msp.ok\"}");
        internal int ExitCode { get; set; }
        internal int ExecuteCalls;
        internal byte[] LastRequest { get; private set; } = [];
        internal byte[] LastPath { get; private set; } = [];
        internal byte[] LastFile { get; private set; } = [];

        private nint AllocateHandle()
        {
            var handle = Marshal.AllocHGlobal(1);
            handles[handle] = 1;
            return handle;
        }

        private void FreeHandle(nint handle)
        {
            if (handles.TryRemove(handle, out _))
            {
                Marshal.FreeHGlobal(handle);
            }
        }

        private byte[] Copy(nint pointer, nuint length)
        {
            if (length == 0)
            {
                return [];
            }

            var result = new byte[checked((int)length)];
            Marshal.Copy(pointer, result, 0, result.Length);
            return result;
        }

        private nint GetResultData(nint result, int index, out nuint length)
        {
            if (!resultBuffers.TryGetValue(result, out var values))
            {
                length = 0;
                return nint.Zero;
            }

            var bytes = index switch
            {
                0 => values.Stdout,
                1 => values.Stderr,
                _ => values.Diagnostic
            };
            if (bytes.Length == 0)
            {
                length = 0;
                return nint.Zero;
            }

            var pointer = Marshal.AllocHGlobal(bytes.Length);
            Marshal.Copy(bytes, 0, pointer, bytes.Length);
            length = (nuint)bytes.Length;
            return pointer;
        }

        public void Dispose()
        {
            if (Interlocked.Exchange(ref disposed, 1) == 0)
            {
                foreach (var handle in handles.Keys)
                {
                    FreeHandle(handle);
                }

                Marshal.FreeHGlobal(versionPointer);
            }
        }
    }

    private sealed class FakeNativeLibrary : IMspCommandRuntimeFfiNativeLibrary
    {
        internal bool IsDisposed { get; private set; }

        public bool TryGetExport(string exportName, out nint address)
        {
            address = nint.Zero;
            return false;
        }

        public void Dispose() => IsDisposed = true;
    }
}

internal sealed class ExplicitCommandRuntimeFfiFactAttribute : FactAttribute
{
    public const string EnvironmentVariable = "READOS_MSP_COMMAND_RUNTIME_FFI_DLL";

    public ExplicitCommandRuntimeFfiFactAttribute()
    {
        if (!OperatingSystem.IsWindows())
        {
            Skip = "The real command runtime FFI is Windows-only in this test suite.";
            return;
        }

        if (string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable(EnvironmentVariable)))
        {
            Skip = $"Set {EnvironmentVariable} to an explicit command runtime FFI DLL path.";
        }
    }
}

# MspFfi

`MspFfi` is a small .NET 8 binding for the public `msp_ffi.h` ABI. It exposes
only opaque session, workspace, and result owners. Workspace paths are virtual
POSIX paths, workspace contents live in the native in-memory workspace, and
session commands are the native registered-command surface; the binding does
not open host paths or launch processes.

## Explicit native loading

The package does not probe the repository, the application directory, or a
runtime-specific folder for `msp_ffi.dll`. An application must explicitly
supply a fully qualified native library path before creating an owner:

```csharp
using MspFfi;

MspRuntime.UseNativeLibrary(nativeDllPath);
using var workspace = MspWorkspace.Create();
using var session = workspace.CreateSession();
using var result = session.Run("echo hello");
result.EnsureSuccess("echo");
Console.WriteLine(Convert.ToHexString(result.StdoutBytes));
```

`MspRuntime.UseNativeLibrary` is intentionally the only native-library
selection mechanism. `MspRuntime.NativeLibraryPathEnvironmentVariable`
(`MSP_FFI_NATIVE_DLL`) is a convention used by the consumer test; the library
does not read it automatically.

A native DLL can be included in a package only when both properties are passed
explicitly, for example:

```powershell
dotnet pack .\src\MspFfi\MspFfi.csproj `
  -p:RuntimeIdentifier=win-x64 `
  -p:MspFfiNativeLibraryPath=C:\explicit\msp_ffi.dll
```

Without `MspFfiNativeLibraryPath`, the package contains no native runtime
asset.

This NuGet opt-in is separate from the ReadOS Windows folder package. ReadOS
builds `native/msp-ffi/Cargo.toml` and includes the verified `msp_ffi.dll` plus
`include/msp_ffi.h` only when `scripts/package-windows.ps1` receives
`-IncludePublicMspFfi`; the default ReadOS package does not carry or load this
public DLL.

## Ownership and bytes

`MspWorkspace`, `MspSession`, and `MspResult` are deterministic `IDisposable`
owners backed by `SafeHandle`. Disposal is idempotent. A session retains its
native workspace reference, so disposing the managed workspace first does not
invalidate an already-created session. Result accessors copy the native
buffers into `byte[]` without text decoding; embedded NULs and non-UTF-8 bytes
are preserved exactly.

All managed string inputs are encoded with strict UTF-8 validation. Embedded
NULs, unpaired UTF-16 surrogates, oversized command/path inputs, and obvious
host-path syntax are rejected before crossing the ABI. File contents are
arbitrary bytes and are not decoded.

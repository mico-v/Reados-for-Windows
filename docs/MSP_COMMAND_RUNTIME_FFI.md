# MSP command runtime FFI boundary

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). This document describes the modular runtime ABI and adapters only.

`native/msp-command-runtime-ffi` is a separate ReadOS-owned C ABI over the deterministic virtual command runtime. It is not the managed `reados-msp-native/1` transport and it does not replace `native/msp-ffi`.

The canonical machine-readable contract is [`native/msp-command-runtime-ffi/abi/msp_command_runtime_ffi.v1.json`](../native/msp-command-runtime-ffi/abi/msp_command_runtime_ffi.v1.json). The header, export definition, Rust exports, .NET adapter, and Android constants are checked against that manifest by `scripts/verify-msp-command-runtime-ffi-abi.ps1`; consumers must not copy a new version or field set without updating the manifest and this document.

## Boundary

The runtime handle owns only `Registry::with_portable_msp_v1()`, the frozen 12-command `reados-portable-msp-v1` profile. The compatibility `posix-core` pack may contain lookup/environment helpers, but those are deliberately excluded from this ABI. The workspace handle owns a mutex-protected `InMemoryWorkspace` and aggregate file counters. No host path, process, environment, `PATH`, callback, policy, audit, or cancellation capability crosses this ABI.

The authored C header uses explicit pointer-and-length arguments and opaque handles. Requests are strict version-1 JSON documents with a required virtual cwd and command, optional canonical RFC 4648 base64 stdin, and explicit UTF-8 variables. Results own bounded binary stdout/stderr and a fixed-code JSON diagnostic until `result_free`.

## Exports

The ABI uses explicit-length UTF-8 JSON requests. Raw NUL bytes in the request are rejected, as are NULs decoded in command, cwd, and variable names or values. Base64-decoded stdin and workspace file data are arbitrary binary and preserve NUL and `0xFF` bytes. Results use length-delimited binary stdout/stderr and fixed-code JSON diagnostics.

The ABI has exactly the 13 names listed in `native/msp-command-runtime-ffi/exports/msp_command_runtime_ffi.def`, all with the complete `msp_command_runtime_ffi_` prefix. There are no aliases for the legacy `msp-ffi` exports.

The managed `ReadOS.Msp.Hosting.Native.RuntimeFfi` echo adapter is an opt-in promotion candidate only. The default `ReadOsMspHost` product construction leaves `ReadOsMspHostDependencies.RuntimeFfiEchoCommandAdapter` null when the packaged artifact is absent or fails validation, so the existing registry composition is unchanged: canonical `pwd`, `echo`, `ls`, and `cat` use the legacy `MspNativeBackedCommand` route and all app commands remain managed. The Windows App startup and package-smoke composition look only beside the application at `AppContext.BaseDirectory\msp_command_runtime_ffi.dll`; they construct the library/adapter only after the exact export, ABI, header-version, architecture, and CRT gates pass. A valid packaged library is transferred to the single host owner, which disposes it exactly once; absent or invalid artifacts fail closed to the old registry. The factory then registers that adapter only under the exact `echo` definition; `pwd`, `ls`, `cat`, and every other command retain their current routes. `MspRuntime` still performs parsing, policy/approval, streaming, cancellation, terminal completion, and the single outer audit record; the adapter contributes no audit records and sends only the virtual `/workspace` cwd, command, optional stdin, variables, and `errorOnUnbound` in its request. The adapter itself rejects case variants and shell operators rather than falling back to a legacy route.

The low-level library/adapter API remains caller-owned. In the Windows App/package-smoke integration, ownership is explicitly transferred to the single host that receives the packaged registration; the registration itself is not independently disposable, preventing DI and the host from releasing the same native library twice.

There is no legacy `msp-ffi` fallback. The Windows package gate now requires the runtime FFI release DLL, its static-CRT/AMD64/exact-export verification, and the Apache license/NOTICE files. At runtime, only a valid DLL beside the application is promoted to the exact `echo` route; missing or invalid artifacts leave the existing registry untouched.

The v1 managed surface has no cancellation parameter: cancellation is explicitly unsupported at this boundary. Callers must serialize lifetime and operation ownership through the adapter's runtime/workspace/result objects and dispose those objects deterministically.

The managed request contract contains only version, command, virtual cwd, optional binary stdin, variables, and `errorOnUnbound`. `environment` is deliberately rejected even when supplied explicitly: the portable ABI has no process/environment authority, and host approval/environment state remains in Hosting. Provider credentials/secrets, actor identity, audit records, policy decisions, host workspace roots, and other host metadata are not accepted as payload fields.


Use the following checks from the repository root:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo package --workspace --list --allow-dirty --locked
cargo test -p msp-command-runtime-ffi --locked
cargo clippy -p msp-command-runtime-ffi --all-targets --locked -- -D warnings
pwsh -NoProfile -File scripts/verify-msp-command-runtime-ffi.ps1
```

The focused release command builds the Windows release DLL with the same static-MSVC-CRT and reproducible-linker settings as the full release verifier (`-C target-feature=+crt-static -C link-arg=/Brepro`, with `CARGO_INCREMENTAL=0`), verifies its PE export directory contains exactly the 13 names in `exports/msp_command_runtime_ffi.def`, compiles the C11 header contract, and runs a C++17 consumer smoke against the DLL. The script owns those build settings; callers do not need to set `RUSTFLAGS`.

The managed adapter contract can be run independently with:

```text
dotnet test tests/ReadOS.Msp.Hosting.Tests/ReadOS.Msp.Hosting.Tests.csproj --no-restore --filter FullyQualifiedName~MspCommandRuntimeFfiAdapterTests
```

This focused test invocation always runs the fake-native contract tests. The one real-DLL test is an explicit fact and skips with an honest message unless `READOS_MSP_COMMAND_RUNTIME_FFI_DLL` is set to a fully qualified Windows release DLL path. The adapter remains opt-in and does not change product routing.

The reviewed Windows release gate remains Windows-local. Android has a
separate arm64-v8a binding and AAR compile/package gate documented in
`docs/ANDROID_RUNTIME_BINDING.md`; the shared portable profile also runs
through a test-only x86_64 Android emulator variant. The Android platform
adapter now projects bounded SAF/ContentResolver document bytes into the
virtual `/workspace/...` namespace, and its API 35 x86_64 instrumentation run
passed 3 tests on 2026-08-26. Arm64 device execution, SAF write-back, and
product workspace integration remain H3.3/H5 acceptance work.

The package is Apache-2.0 licensed with a separate `NOTICE`; it has no default feature that changes ABI behavior.

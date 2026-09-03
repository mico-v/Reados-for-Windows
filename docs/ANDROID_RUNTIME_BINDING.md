# Android runtime binding

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). This binding proves the portable ABI package boundary, the first SAF workspace projection, and the H4.3 fixed-profile tool-provider experiment; it is not a complete Android product host or Termux integration.

## Scope

`native/msp-command-runtime-ffi/android` is a real Android library project for
`com.reados.msp.commandruntime`. It builds a Kotlin API, a C++17 JNI shim, and
an AAR containing the arm64-v8a Rust runtime artifact. The project is
ReadOS-authored glue around the adjacent public C ABI; it does not import raw
`MSP/` source.

The pinned build toolchain is Gradle 8.9, Android Gradle Plugin 8.7.3, Kotlin
2.0.21, JDK 17, compileSdk 35, minSdk 24, NDK 27.2.12479018, and CMake 3.22.1.
The NDK matches the existing portable workflow's Android target job. The
module currently packages only `arm64-v8a`.

The release AAR is arm64-v8a-only. A test-only Gradle ABI override can package
an x86_64 artifact for emulator instrumentation; it does not widen the
published AAR profile.

## SAF workspace projection

`MspAndroidContentResolverWorkspaceSource` is the first real Android workspace
adapter. It accepts an already-authorized SAF tree `content://` `Uri`, walks
children through `DocumentsContract`/`ContentResolver`, and reads bounded
document bytes. The adapter maps only `/workspace/<safe-name>/...` virtual
paths; URI authorities and provider document IDs remain Kotlin-owned and never
enter Rust requests, diagnostics, logs, or audit records. `.msp`, traversal,
host separators, control characters, and ambiguous duplicate child names are
rejected. Provider failures are reduced to path-free typed categories.

`MspAndroidWorkspaceProjection` copies one bounded document into the existing
Rust in-memory workspace before executing a command. This is deliberately a
projected temporary workspace, not a host-path bridge or a claim that Rust can
open Android storage directly. Cancellation is wired to both a cooperative
token and Android `CancellationSignal`; closing the source cancels active
operations and later calls fail as `Closed`. Policy, approval, audit,
credential, and process authority remain in the host and are not added to the
Android adapter.

## Native boundary

The JNI shim calls exactly the 13 exports in
`native/msp-command-runtime-ffi/exports/msp_command_runtime_ffi.def`:

- ABI/version metadata;
- runtime create/free;
- workspace create/free/put-file;
- JSON execution; and
- result exit-code, stdout, stderr, diagnostic, and free operations.

Every pointer/length pair is explicit. JNI validates array lengths, copies
input arrays before the call, and converts result pointers plus `size_t` lengths
into `MspFfiByteBuffer` values. Kotlin result values make defensive copies.
There is no NUL-terminated string assumption for binary data.

The API contains only virtual cwd/path, command, optional binary stdin,
variables, and `errorOnUnbound`. It has no host path, process, environment or
PATH, policy, audit, provider, actor, or cancellation field.

## Loading, validation, and ownership

Loading is explicit:

```kotlin
val bridge = MspCommandRuntimeFfiNative.load()
MspCommandRuntimeFfiClient(bridge).openSession().use { session ->
    val result = session.execute(MspVirtualRequest("pwd", "/workspace"))
}
```

`load()` calls `System.loadLibrary` for the Rust library and then the JNI shim,
never searches a host path, and checks ABI `1` and version `0.1.0` before
returning. A mismatch throws `MspCommandRuntimeFfiAbiException`; no fallback
library is attempted.

`MspCommandRuntimeFfiSession` owns runtime/workspace handles. Explicit
`AutoCloseable` runtime, workspace, and result wrappers free each native handle
exactly once. Results are independent of runtime/workspace and can be detached
before those owners close. Native status values and JNI argument failures are
mapped to typed Kotlin exceptions/errors without forwarding native paths or
arbitrary native text.

## Rust artifact integration

The Android build has no fake Rust implementation. Build the release artifact
first:

```text
cargo build --manifest-path native/msp-command-runtime-ffi/Cargo.toml \
  --target aarch64-linux-android --release --locked
```

Pass the resulting
`target/aarch64-linux-android/release/libmsp_command_runtime_ffi.so` as
`-PrustFfiArtifact=...` (or set
`READOS_MSP_COMMAND_RUNTIME_FFI_ANDROID_SO`). Gradle copies it to the AAR's
`jni/arm64-v8a` directory, checks all 13 `msp_command_runtime_ffi_` symbols
with NDK `llvm-nm`, and links the JNI shim against that exact artifact. A
missing artifact or export drift fails the build.

The Rust Android cdylib carries the stable SONAME
`libmsp_command_runtime_ffi.so`. The JNI shim therefore records a loader-safe
library name in `DT_NEEDED`; it never records the build-machine path used by
Gradle/CMake to locate the input artifact.

## Shared profile instrumentation

The Android test source reads the canonical
`native/msp-command-pack/profile/portable_msp_v1_fixtures.json` asset, writes
the fixture workspace through the virtual workspace handle, and executes the
portable command cases through the real JNI/Rust ABI. It compares binary
stdout/stderr and exit codes, rejects the excluded `command` helper, and
checks that diagnostics do not disclose host paths or environment values.

The portable workflow runs this test on an API 35 x86_64 emulator using a
test-only `x86_64-linux-android` artifact. Local evidence on 2026-08-26:
the portable-profile test passed as part of the 3-test
`connectedDebugAndroidTest` run on `reados-api35-x86_64(AVD) - 15`. An arm64
AVD cannot run on this x86_64 Windows host; arm64 device/emulator evidence
remains the H3.3 requirement.

The real workspace instrumentation uses a test-only in-memory
`DocumentsProvider` with the same `ContentResolver`/SAF APIs. On 2026-08-26,
the API 35 x86_64 emulator ran
`connectedDebugAndroidTest` successfully: 3 tests passed, including binary
document projection through virtual `/workspace/doc.bin` into Rust `cat`,
cancellation, close, and host-disclosure checks. The provider manifest is
explicitly mapped into the androidTest source set and protected by
`MANAGE_DOCUMENTS`, so this result exercises the provider boundary rather than
an in-memory Kotlin fake source.

This evidence is x86_64 emulator-only. The published arm64-v8a AAR is still
compile/package verified, but an arm64 device/emulator run was not possible on
this x86_64 host (the Android emulator reports that arm64 AVDs are unsupported
by the local QEMU configuration). Real user-selected SAF permission lifecycle,
write-back, directory mutation, and product policy routing remain future work.

## CI and Windows impact

`.github/workflows/portable-rust-ci.yml` installs Java 17, NDK
27.2.12479018, and CMake 3.22.1 in the Ubuntu `android-check` job, builds the
Rust `aarch64-linux-android` `.so`, then runs the Android Gradle Kotlin tests
and `verifyReleaseAar` task:

```text
native/msp-command-runtime-ffi/android/gradlew --no-daemon \
  :msp-command-runtime-ffi:test \
  :msp-command-runtime-ffi:verifyReleaseAar \
  -PrustFfiArtifact=...
```

`verifyReleaseAar` depends on `assembleRelease`; it checks the AAR's
`classes.jar`, both `arm64-v8a` shared objects, and the exact 13 Rust exports
inside the packaged Rust `.so`. The check is a compile/package smoke only; the
separate emulator command is the semantic instrumentation evidence.

The existing Windows workflow remains Windows-only and does not invoke Gradle,
Android SDK/NDK, CMake, or the Android project. Cargo workspace membership is
unchanged, so ordinary Windows builds have no Android prerequisite or output.

## H4.3 tool-provider experiment

`MspAndroidToolProvider` is the Android platform-layer experiment described in
[ANDROID_TOOL_PROVIDER_DECISION.md](ANDROID_TOOL_PROVIDER_DECISION.md). It
selects app-owned Toybox as the first production direction, with fixed `pwd`,
`cat`, and `ls` profiles over an app-private projected workspace. The request
contains only a profile and virtual paths. It clears the environment, pins the
executable digest, bounds output/time, terminates on cancellation, and
redacts bound paths. `MspAndroidToolProviderFactory` is the consuming-app
binding: it requires the app to supply a reviewed `toybox` asset and expected
SHA-256, extracts it below `noBackupFilesDir`, verifies the manifest,
provenance, NOTICE, and executable identity, and otherwise returns an explicit
failure. It never falls back to `/system/bin/toybox`, PATH, or Termux. The
provider manifest and provenance live under
`native/msp-command-runtime-ffi/android/provider/` and the metadata is shipped
in the AAR assets; the executable itself is intentionally not shipped yet.

`SystemToyboxOracle` instrumentation runs against `/system/bin/toybox` only as
device evidence. `AppOwnedBundle` is not enabled by this repository because no
ReadOS-distributed Toybox artifact is present yet. `TermuxOptional` fails
closed, so this experiment never creates a Termux shell session or consults
the Android host `PATH`. On 2026-08-26, the Gradle unit tests, provider
contract tests, `assembleDebugAndroidTest`, and `verifyReleaseAar` passed with
the real x86_64-linux-android Rust artifact. The API 35
`reados-api35-x86_64(AVD) - 15` emulator then ran all 7
`connectedDebugAndroidTest` tests successfully, including app-owned binding
failure when no Toybox asset is present, system Toybox oracle reads, projected
workspace reads, cancellation, and path-redaction checks.

The Host/App side now exposes an optional
`MspVerifiedRuntimeProviderCatalog`, and the host-neutral contract has a
Toybox runtime family plus fixed argv prefixes for multi-call profiles. This
is registration/launch-plan authority only; Android process execution remains
in the Kotlin provider and product policy/approval/audit remain in the host.

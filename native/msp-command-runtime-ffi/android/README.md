# Android command-runtime FFI library

This directory is a self-contained Android library project for the adjacent
`native/msp-command-runtime-ffi` C ABI. It is no longer a source-only
contract. The project builds an AAR containing:

- Kotlin API in `com.reados.msp.commandruntime`;
- `libmsp_command_runtime_ffi_jni.so`, a C++17 JNI shim; and
- the real Rust `libmsp_command_runtime_ffi.so` under `jni/arm64-v8a`.

The release AAR is deliberately `arm64-v8a` only. For emulator
instrumentation, Gradle can be explicitly given `-PandroidAbi=x86_64`,
`-PrustTarget=x86_64-linux-android`, and a matching Rust artifact; that is a
test-only packaging variant and is not part of the published AAR contract.
The API accepts only virtual command values and binary buffers. There are no host paths, process handles,
environment or PATH lookups, policy/audit fields, or cancellation fields.

## Toolchain pinned by this module

| Tool | Version |
| --- | --- |
| Gradle | 8.9 |
| Android Gradle Plugin | 8.7.3 |
| Kotlin Android plugin | 2.0.21 |
| JDK | 17 |
| compileSdk | 35 |
| minSdk | 24 |
| Android NDK | 27.2.12479018 |
| CMake | 3.22.1 |
| Release Rust target | `aarch64-linux-android` |
| Android API passed to Rust linker | 24 |

AGP 8.7.3 is paired with Gradle 8.9 and JDK 17. The NDK is the same
`27.2.12479018` used by `.github/workflows/portable-rust-ci.yml`, so the Rust
linker and CMake toolchain do not drift between the portable Rust and Android
AAR jobs.

## Build from the repository root

Install Android SDK platform 35, build-tools 35.0.0, NDK 27.2.12479018, CMake
3.22.1, JDK 17, Rust, and the `aarch64-linux-android` Rust target. Then run:

```bash
cargo build --manifest-path native/msp-command-runtime-ffi/Cargo.toml \
  --target aarch64-linux-android --release --locked
native/msp-command-runtime-ffi/android/gradlew \
  -p native/msp-command-runtime-ffi/android \
  :msp-command-runtime-ffi:assembleRelease \
  -PrustFfiArtifact="$PWD/target/aarch64-linux-android/release/libmsp_command_runtime_ffi.so"
```

The Gradle task copies the supplied `.so` into
`msp-command-runtime-ffi/build/generated/rust-jniLibs/arm64-v8a/`, checks its
13 required C ABI symbols with `llvm-nm`, links the JNI shim against that exact
file, and packages both libraries in the AAR. If the artifact is missing, the
build fails; there is no fake fallback or host-library fallback.

The Rust artifact embeds the stable Android SONAME
`libmsp_command_runtime_ffi.so`; the JNI shim links by that SONAME rather than
by a build-machine absolute path.

The artifact can also be supplied through the
`READOS_MSP_COMMAND_RUNTIME_FFI_ANDROID_SO` environment variable or the
`rustFfiArtifact` Gradle property. This path is a build input only, not an API
field and not an Android runtime path lookup.

## Fixed-profile Android tool provider

The AAR also carries the Toybox provider metadata assets:
`android-tool-provider-v1.json`, `provenance.json`, and `NOTICE`. It does not
carry a Toybox executable. A consuming Android app must add a reviewed asset
named `toybox` and pass its expected SHA-256 to
`MspAndroidToolProviderFactory.bindAppOwnedBundle(...)`. The factory extracts
the asset below the app's private `noBackupFilesDir`, verifies metadata and
the executable digest, and returns an explicit unavailable/identity failure
when anything is missing or changed. It never uses `/system/bin/toybox`, PATH,
BusyBox, or Termux as a fallback.

The only profiles are `pwd`, `cat`, and `ls`; each has a fixed Toybox argv
prefix and accepts only bounded virtual-workspace suffix paths. The factory
and provider own Android process/storage mechanics; policy, approval, audit,
and product routing remain in the consuming Host.

## Runtime loading and ownership

Call `MspCommandRuntimeFfiNative.load()` explicitly. It loads
`msp_command_runtime_ffi` first and `msp_command_runtime_ffi_jni` second, then
checks both ABI `1` and version `0.1.0`. It throws
`MspCommandRuntimeFfiAbiException` on mismatch. The default
`MspCommandRuntimeFfiClient()` remains intentionally unavailable and never
loads a library.

For deterministic ownership, use:

```kotlin
val bridge = MspCommandRuntimeFfiNative.load()
bridge.openSession().use { session ->
    val result = session.execute(
        MspVirtualRequest(command = "pwd", cwd = "/workspace")
    )
    // result is detached; stdout/stderr/diagnostic are defensive copies.
}
```

`MspCommandRuntimeFfiSession` owns one runtime and one workspace. Result,
runtime, and workspace handles are idempotent `AutoCloseable` objects. Results
copy all length-delimited bytes before a native result is released. JNI copies
input arrays before calling Rust and passes explicit lengths; it never retains
JVM arrays.

The JNI shim calls the exact 13 functions listed in
`../exports/msp_command_runtime_ffi.def`. JNI argument/buffer violations map
to `IllegalArgumentException`/`IllegalStateException`; nonzero workspace
statuses map to `MspCommandRuntimeFfiNativeStatusException`. No native error
path, process output, or host metadata is surfaced.

## Shared profile instrumentation

`src/androidTest/kotlin/.../MspPortableProfileInstrumentationTest.kt` reads
the canonical `native/msp-command-pack/profile/portable_msp_v1_fixtures.json`
from the test APK, projects its workspace through the JNI session, and checks
the same command outputs, binary bytes, exit codes, excluded-helper behavior,
and disclosure rules. It never invokes a shell or a host path.

Run it on an x86_64 emulator with a test-only Rust artifact:

```bash
cargo build --manifest-path native/msp-command-runtime-ffi/Cargo.toml \
  --target x86_64-linux-android --release --locked
native/msp-command-runtime-ffi/android/gradlew \
  -p native/msp-command-runtime-ffi/android \
  :msp-command-runtime-ffi:connectedDebugAndroidTest \
  -PandroidAbi=x86_64 \
  -PrustTarget=x86_64-linux-android \
  -PrustFfiArtifact="$PWD/target/x86_64-linux-android/release/libmsp_command_runtime_ffi.so"
```

The x86_64 emulator variant proves that the portable semantics execute inside
Android's runtime. It does not claim arm64 hardware/emulator evidence; the
release AAR remains arm64-v8a. H3.3 now also includes a read-only
`ContentResolver`/SAF adapter that projects bounded provider bytes into the
virtual `/workspace` namespace; arm64 device evidence, SAF write-back, and
product workspace adoption remain open.

## CI and Windows isolation

The portable workflow builds this only in its Ubuntu `android-check` job. The
existing Windows workflow does not include this Gradle project, and Cargo's
workspace remains unchanged. Therefore Windows builds do not require Gradle,
Android SDK, NDK, CMake, or an Android Rust target. The portable workflow runs
the x86_64 emulator fixture in addition to the arm64 compile/AAR gate.

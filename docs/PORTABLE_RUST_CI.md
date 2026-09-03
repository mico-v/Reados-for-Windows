# Portable Rust CI

`portable-rust-ci.yml` provides the portable Rust gates without changing the
existing Windows workflow or any runtime routing.

## Linux host scope

The `rust-workspace` job runs on `ubuntu-latest` and is the neutral Rust
workspace gate. It checks formatting, runs workspace tests, explicitly runs
`msp-backend-linux` fixture tests, runs Clippy with warnings denied, packages
every workspace member without verification, and checks/builds target-neutral
workspace crates for `x86_64-unknown-linux-gnu`. The Linux target check and
build exclude `msp-backend-windows`; that adapter is Windows-only and is verified
by the Windows workflow/local Windows evidence. The Linux backend test is runtime
evidence for the owned-root/openat2 boundary. No Windows backend behavior is
inferred from a Linux build.

## Android scope

The `android-check` job installs Java 17, Android SDK platform 35/build tools
35.0.0, platform-tools, emulator, the API 35 x86_64 Google APIs system image,
CMake 3.22.1, and NDK `27.2.12479018`, verifies both AArch64 and x86_64 NDK
Android clang/linker tools, configures Cargo and C compilation tools, and runs:

```text
cargo check --manifest-path native/msp-command-runtime-ffi/Cargo.toml --target aarch64-linux-android --locked
cargo build --manifest-path native/msp-command-runtime-ffi/Cargo.toml --target aarch64-linux-android --release --locked
cargo build --manifest-path native/msp-command-runtime-ffi/Cargo.toml --target x86_64-linux-android --release --locked
native/msp-command-runtime-ffi/android/gradlew --no-daemon \
  :msp-command-runtime-ffi:connectedDebugAndroidTest \
  -PandroidAbi=x86_64 -PrustTarget=x86_64-linux-android \
  -PrustFfiArtifact=$GITHUB_WORKSPACE/target/x86_64-linux-android/release/libmsp_command_runtime_ffi.so
native/msp-command-runtime-ffi/android/gradlew --no-daemon \
  :msp-command-runtime-ffi:test :msp-command-runtime-ffi:compileDebugAndroidTestKotlin \
  :msp-command-runtime-ffi:assembleRelease \
  -PrustFfiArtifact=$GITHUB_WORKSPACE/target/aarch64-linux-android/release/libmsp_command_runtime_ffi.so
```

The Android Gradle library is pinned to Gradle 8.9, Android Gradle Plugin
8.7.3, Kotlin 2.0.21, compileSdk 35, minSdk 24, NDK 27.2.12479018, and CMake
3.22.1. It compiles the Kotlin/JNI module, checks all 13 Rust C ABI exports
with NDK `llvm-nm`, links the JNI shim against the runner-built Rust `.so`, and
packages a real arm64-v8a AAR. Missing artifacts or export drift fail the job;
no fallback library is supplied. The x86_64 emulator instrumentation reads
the canonical portable-profile fixture asset and executes it through the real
Rust/JNI ABI. The local H3.3 instrumentation additionally runs the test-only
`DocumentsProvider` through `ContentResolver` and verifies bounded SAF
projection into `/workspace`, cancellation, close, and disclosure boundaries.
The x86_64 artifact is test-only and is not packaged into the published AAR.

The arm64 release path remains compile/package validation. The separate x86_64
emulator step is semantic Android runtime evidence; it does not claim an arm64
device run. The local H3.3 adapter evidence separately exercises a real
`ContentResolver`/`DocumentsProvider` projection into `/workspace`, but does
not claim SAF write-back or product workspace adoption.

## Blocked local targets and evidence

This checkout may still lack local Windows target toolchains or other
platform-specific prerequisites. Until CI passes, local Windows missing targets
remain blocked and the result must be independently reviewed. The portable
workflow does not automatically promote or rewrite capability-manifest
evidence, release metadata, or any other provenance record.

The workflow and this document do not touch raw `MSP/` sources, the legacy MSP
ABI, or managed routing. The existing Windows CI remains the authority for
Windows-specific and Windows runtime checks.

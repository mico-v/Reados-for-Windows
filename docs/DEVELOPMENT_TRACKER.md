# ReadOS Development Tracker

Updated: 2026-08-26.

This is the authoritative active item tracker. Architecture belongs in [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md); phase details belong in [../DEVELOPMENT_PLAN.md](../DEVELOPMENT_PLAN.md). Historical T1-T95 execution prose was removed because it duplicated plans, contained conflicting statuses, and obscured the current convergence work.

## Tracking Rules

- Use only `planned`, `in progress`, `done`, or `blocked`.
- A `done` item records exact acceptance evidence and remaining limitations.
- Compilation alone does not prove a platform capability.
- Component tests alone do not close a product acceptance item.
- Do not add new generic features to legacy `native/msp-core` except security, compatibility, packaging, or migration-enabling fixes.
- Update this file in the same change that materially changes scope or status.

## Current Verified Local Baseline

Observed on the current working tree on 2026-08-26:

- `dotnet build ReadOS.sln --no-restore`: 0 warnings, 0 errors.
- managed tests: 69 Core + 421 Hosting + 425 App passed; six explicit real-DLL tests skipped because release DLL environment variables were not configured.
- modular root Rust workspace tests passed.
- legacy `native/msp-core`: 301 unit + 7 PTY integration tests passed.
- optional public `native/msp-ffi`: 7 tests passed.

This is a local diagnostic baseline, not a claim that the full release/package gate or every target-platform capability passed on this date.

## Milestone Status

| Milestone | Status | Current result | Exit gate |
| --- | --- | --- | --- |
| H0 Documentation and governance reset | done | One product goal, hybrid architecture, development plan, active tracker, and short goal prompt | Repository links reference only current authority documents |
| H1 Legacy capability inventory and freeze | planned | Legacy and modular runtimes still overlap | Complete migration matrix and legacy feature-freeze enforcement |
| H2 Portable runtime contract convergence | in progress | ABI and frozen 12-command profile are verified on Windows, Linux, and Android x86_64 emulator; Android arm64 storage remains separate | One stable portable ABI and command profile across target hosts |
| H3 Real platform workspace backends | in progress | Linux host backend, modular Windows retained-handle read/write, and Android SAF projection slices exist; arm64 Android device evidence remains open | Common real-workspace contract passes on Windows, Linux, Android |
| H4 Verified external runtime providers | in progress | H4.1 contract and H4.2 fixed read-only Git provider/backend acceptance slice are complete; managed provider adoption, packaged executable policy, and confinement hardening remain | Same registered external command profile runs safely through the supported Host on Windows and one non-Windows target |
| H5 Product adoption of modular runtime | in progress | Optional modular Runtime FFI adoption is bounded; legacy adapter remains packaged | Supported product commands use one modular Hosting adapter |
| H6 Product-level acceptance | in progress | Service-level flagship and failure branches exist | Real provider plus packaged-process restart plus visible UI evidence |
| H7 Legacy runtime retirement | planned | Legacy core/FFI remain required for current release evidence | Package, product, tests, and docs pass without legacy artifacts |
| H8 Multi-platform SDK profiles | planned | Windows product, Linux backend, Android AAR pieces exist separately | Supported versioned Windows/Linux/Android profiles and release evidence |

## Immediate Backlog

### H1.1 Runtime migration matrix

Status: done.

Create `docs/MSP_RUNTIME_MIGRATION_MATRIX.md`. Inventory every module and public capability in `native/msp-core`, `native/msp-ffi`, the modular workspace, Hosting native adapters, packaging scripts, and CI. Classify each legacy capability as `port`, `adapt`, `defer`, or `delete`, with destination, owner, acceptance test, and retirement dependency.

Acceptance:

- every legacy source module has one disposition;
- every duplicate capability has one named authoritative target;
- product/package dependencies on legacy artifacts are explicit.

Evidence:

- `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-msp-runtime-migration-matrix.ps1` passed on 2026-08-26.
- 102 scoped legacy source files are covered by 46 disposition rows with no unmapped or ambiguous matches.
- The matrix records portable behavior as `port`, platform/ABI/provider migration as `adapt`, future external-runtime work as `defer`, and product-specific or obsolete code as `delete`.

Limitations:

- The matrix is an ownership and retirement inventory; it does not claim that any `port` or `adapt` row is already implemented in the modular destination.
- Generated `target/` output and private local state are intentionally outside the source inventory.

### H1.2 Enforce the legacy feature freeze

Status: done.

Add a verification rule and repository guidance preventing new product references to raw legacy FFI exports and preventing new general-purpose legacy modules without an explicit migration exception.

Acceptance:

- CI or a focused verification script detects new forbidden dependencies;
- security and migration exceptions are documented in the tracker.

Evidence:

- `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-msp-legacy-boundary.ps1` passed on 2026-08-26.
- The verifier invokes the H1.1 matrix gate, which covers 102 legacy source files with 46 explicit disposition rows; an added legacy source/module must be registered before the gate can pass.
- Product roots (`src/ReadOS.App`, `src/ReadOS.Msp`, and non-native `src/ReadOS.Msp.Hosting`) contain no direct legacy DLL, C ABI export, `DllImport`, or `NativeLibrary.Load/Free` references. The retained `src/ReadOS.Msp.Hosting/Native` adapter is the sole allowed legacy compatibility boundary.
- `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-msp.ps1 -SkipNative` passed: Core 69, Hosting 408 passed/6 explicit DLL skips, App 425, and solution build 0 warnings/0 errors.

Limitations:

- The rule intentionally allows legacy references inside the Hosting native adapter, compatibility tests, packaging scripts, and CI because the legacy runtime is still required until H7.
- This gate proves dependency ownership and registration discipline; it does not prove that the legacy runtime can be retired or that target-platform native capabilities are complete.

### H2.1 Select the canonical modular ABI

Status: done.

Reconcile the current `msp-command-runtime-ffi` contract with required .NET and Android consumers. Freeze version, exports, request/result ownership, limits, binary behavior, diagnostics, and compatibility policy.

Acceptance:

- one versioned ABI document and generated/verified export list;
- fake-native and real-library malformed-input/ownership tests;
- .NET and Kotlin adapters consume the same contract;
- no policy, audit, actor, provider, host path, or process authority crosses the ABI.

Evidence:

- `native/msp-command-runtime-ffi/abi/msp_command_runtime_ffi.v1.json` is the canonical v1 manifest. `scripts/verify-msp-command-runtime-ffi-abi.ps1` passed on 2026-08-26 and matched 13 exports, 14 limits, 5 status codes, request fields, and forbidden authority fields across the C header, `.def`, Rust, .NET, and Android sources.
- Rust FFI tests passed: 10 unit tests plus 4 exported-surface integration tests. The new `host_environment_field_is_rejected_at_the_abi_boundary` test proves `environment` cannot be smuggled into the portable request.
- .NET Hosting FFI contract tests passed: 16 plus 1 explicit real-DLL metadata test skipped when `READOS_MSP_COMMAND_RUNTIME_FFI_DLL` was not configured; the same focused run passed all 17 with the rebuilt release DLL supplied explicitly. Fake-native tests cover malformed UTF-8/NUL/oversized/non-virtual/undeclared requests, cross-library handles, binary buffers, and module/result ownership.
- `scripts/verify-msp-command-runtime-ffi.ps1` passed on the rebuilt Windows release DLL with static CRT and reproducible-linker flags (`-C target-feature=+crt-static -C link-arg=/Brepro`): exact 13 PE exports, C11 header smoke, and C++17 real-DLL smoke. The real smoke covers malformed JSON and result independence after runtime/workspace release.
- `cargo fmt --manifest-path native/msp-command-runtime-ffi/Cargo.toml -- --check`, `cargo test -p msp-command-runtime-ffi --locked`, and `cargo clippy -p msp-command-runtime-ffi --all-targets --locked -- -D warnings` passed. Managed verification passed with Core 69, Hosting 408 plus 6 explicit DLL skips, App 425, and solution build 0 warnings/0 errors.

Limitations:

- The ABI remains a deterministic virtual-runtime boundary; process, PTY, host filesystem, host environment, policy, approval, audit, and cancellation authority are intentionally outside it. A future provider must carry those capabilities through the Host contract, not by widening this request.
- Android evidence includes source/manifest alignment, the arm64 JNI/AAR gate,
  and the H2.2 x86_64 emulator fixture run; no arm64 device execution is
  claimed here.
- Full workspace `cargo fmt --all -- --check` still reports a pre-existing formatting difference in `native/msp-command-pack/src/lib.rs`; this slice did not rewrite that user-owned change.

### H2.2 Freeze the portable builtin profile

Status: done.

Review the current command pack and define the supported MSP subset. Stop adding commands without a product workflow or conformance requirement.

Acceptance:

- each command documents options, effects, limits, deviations, and binary behavior;
- a shared fixture suite runs on Windows, Linux, and Android hosts;
- unsupported options fail deterministically without host disclosure.

Evidence:

- `native/msp-command-pack/profile/portable_msp_v1.json` freezes the
  `reados-portable-msp-v1` profile at exactly 12 read-only virtual-workspace
  commands: `cat`, `du`, `echo`, `find`, `grep`, `head`, `ls`, `printf`, `pwd`,
  `sed`, `tail`, and `wc`. The compatibility-only `command`, `env`, `type`,
  and `which` helpers remain outside the modular FFI profile.
- Every portable command has documented options, effects, limits, deviations,
  and binary behavior in the manifest. The 17-case shared fixture file covers
  all 12 commands, binary stdin, and four deterministic unsupported-option
  cases with fixed exit code 2 diagnostics. An explicit `command` fixture
  verifies that an excluded compatibility helper is not accidentally exposed
  by the frozen profile.
- `powershell -NoProfile -ExecutionPolicy Bypass -File
  .\scripts\verify-msp-command-profile.ps1` passed on 2026-08-26. The verifier
  checks manifest/Rust registry/FFI selection consistency, fixture coverage,
  required documentation fields, package inclusion, unsupported-option count,
  host-disclosure tokens, Android instrumentation fixture wiring, and stable
  Android SONAME linkage markers.
- `cargo test -p msp-command-pack --locked` passed on Windows: 57 unit tests,
  2 portable-profile integration tests, and 0 doc tests. The same command ran
  under WSL Arch Linux and passed with the same 57 + 2 test counts, executing
  the shared fixtures on a Linux host rather than merely cross-compiling.
- `powershell -NoProfile -ExecutionPolicy Bypass -File
  .\scripts\verify-msp-command-runtime-ffi.ps1 -SkipBuild` passed on the
  existing Windows release DLL: ABI manifest verification, portable-profile
  verification, exact 13-export inspection, C11 header smoke, and C++ runtime
  smoke. Because `-SkipBuild` was used, this result does not claim a fresh
  static-CRT/reproducible release build.
- `cargo clippy -p msp-command-pack --all-targets --locked -- -D warnings`
  passed, and `cargo package -p msp-command-pack --list --allow-dirty --locked`
  listed both profile manifests in the package contents.
- Android instrumentation is now wired to the canonical fixture asset rather
  than a copied Kotlin command list. The JNI/Rust linkage was corrected to use
  the stable `libmsp_command_runtime_ffi.so` SONAME instead of embedding the
  Gradle build-machine path in `DT_NEEDED`. The default arm64 release AAR,
  `compileDebugAndroidTestKotlin`, unit tests, and `verifyReleaseAar` all passed
  after this fix.
- On 2026-08-26, an API 35 x86_64 Android emulator ran
  `:msp-command-runtime-ffi:connectedDebugAndroidTest` successfully: 1 test
  passed. The test executed the 16 portable-profile cases (all 12 commands
  plus four unsupported-option cases), checked binary output/exit codes and
  disclosure rules, and separately verified that the excluded `command`
  helper was not exposed. The emulator used a test-only
  `x86_64-linux-android` Rust artifact; the published AAR remains arm64-v8a.

Limitations:

- Android semantic evidence is from the API 35 x86_64 emulator test-only
  variant. An arm64 AVD cannot run on this x86_64 Windows host, so this does
  not claim arm64 hardware/emulator evidence. The arm64 release AAR remains
  compile/package verified; arm64 device execution and SAF write/product
  integration are tracked separately under H3.3/H5. The H3.3 read-only
  ContentResolver projection is covered by its own evidence below.
- Full workspace `cargo fmt --all -- --check` still reports a pre-existing
  formatting difference in `native/msp-command-pack/src/lib.rs`; this slice
  did not rewrite that user-owned change.

### H3.1 Modular Windows retained-handle workspace backend

Status: done.

Port/adapt safe Windows workspace behavior from legacy `msp-core` behind `msp-backend-windows` or its reviewed bridge. Do not restore pathname-based check-then-open implementations.

Acceptance:

- retained root/target handle containment;
- stat/list/range-read plus approved write contract;
- drive/UNC/device/ADS/reparse/hidden-path negative tests;
- no legacy types in the public modular backend API;
- real Windows integration evidence.

Evidence:

- `native/msp-backend-windows/src/platform.rs` now owns the Windows implementation. It retains a
  fixed local NTFS root handle, opens target handles first, verifies final-handle containment with
  ordinal case-insensitive component boundaries, revalidates final components, rejects reparse
  points and hidden `.msp`, and uses the verified handle for every operation.
- The neutral `WorkspaceBackend` surface supports bounded `stat`, deterministic `list`, binary
  `read_range`, and bounded whole-file replace-or-create `write_file`. Process, PTY, usage,
  streaming, and cancellation remain explicit unsupported capabilities; policy/approval/audit stay
  above the backend.
- Focused Windows evidence on 2026-08-26: `cargo test -p msp-backend-windows --all-features --locked`
  passed 10 tests, including binary nested read/write, deterministic listing, bounds, hidden path,
  ADS/device/UNC syntax, outward reparse escape (skips only when the host denies symlink creation),
  and path-free diagnostics. `cargo clippy -p msp-backend-windows --all-features --all-targets
  --locked -- -D warnings` passed.
- The public modular API exposes only neutral DTOs and `WorkspaceBackend`; no legacy `msp-core`
  type or handle crosses the crate boundary. `cargo check -p msp-backend-windows
  --all-features --target x86_64-pc-windows-msvc --locked` is covered by the same Windows target.

Limitations:

- The approved write primitive is intentionally only whole-file replace-or-create beneath an
  existing verified parent. Directory creation, rename, delete, recoverable trash, usage, and
  product policy/write routing remain later slices.
- The backend is Windows-only; non-Windows builds fail closed. This does not claim Linux/Android
  parity, arm64 evidence, process/PTY support, or product adoption of `ls`/`cat`.

### H3.2 Linux common workspace contract

Status: done.

Run and extend the common workspace suite against the `openat2` backend. Close remaining write/rename/delete/capability gaps without unsafe fallback.

Acceptance:

- Ubuntu runtime evidence for all claimed operations;
- unsupported-kernel behavior remains fail-closed;
- symlink, special-file, traversal, `.msp`, and path-disclosure tests pass.

Evidence (2026-08-26):

- `native/msp-backend-linux` now implements the common read/write slice plus bounded
  descriptor-relative `renameat2` and `unlinkat` mutation. Rename supports regular files and
  directory subtrees with optional no-replace semantics; delete supports regular files and empty
  directories only. Recursive delete, trash, directory creation, and metadata-preserving copy are
  intentionally unsupported. Policy, approval, audit, and recoverable-trash ownership remain in
  the host layer.
- The Linux backend keeps an owned root descriptor and uses `openat2` with
  `RESOLVE_BENEATH|RESOLVE_NO_MAGICLINKS|RESOLVE_NO_SYMLINKS` for every target/parent. There is no
  canonicalize/check/reopen fallback. `ENOSYS` and `EINVAL` during the `openat2` probe classify as
  typed `UnsupportedPlatform`; other probe failures classify as path-free `Operation`.
- Focused Windows-checkout verification: `cargo test -p msp-backend-linux --locked` passes the
  non-Linux stub suite (2 tests); `cargo clippy -p msp-backend-linux --all-targets --locked --
  -D warnings` passes. The same crate was executed on WSL2 Arch Linux on 2026-08-26:
  `cargo test -p msp-backend-linux --locked` passed 9 tests covering binary nested stat/list/range,
  bounded anchored write, descriptor-relative rename/delete, collision and non-empty-directory
  behavior, symlink/special-file escape rejection, hidden/traversal rejection, path-free errors,
  and unsupported-kernel classification; Linux Clippy passed with `-D warnings`.
- Ubuntu 24.04.4 LTS container runtime evidence is now present. The current worktree was mounted
  into image digest `sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517`
  (`docker.m.daocloud.io/library/ubuntu:24.04`) with Rust 1.93.1, `build-essential`, and the
  locked Cargo registry. `cargo test -p msp-backend-linux --locked` passed all 9 Linux runtime
  tests; `cargo clippy -p msp-backend-linux --all-targets --locked -- -D warnings`,
  `cargo check --workspace --target x86_64-unknown-linux-gnu --locked --exclude
  msp-backend-windows`, and package-content checks passed. The same Ubuntu container also ran
  `cargo test --workspace --locked`; every workspace test binary passed, including the 9 Linux
  backend tests.
- `.github/workflows/portable-rust-ci.yml` remains the reproducible hosted gate: `ubuntu-latest`
  runs the focused Linux tests, workspace tests, Clippy, package listing, and Linux target build.
  The local Ubuntu container result closes the H3.2 target-runtime evidence requirement without
  claiming that a GitHub Actions run occurred.

Limitations:

- Linux rename/delete are physical backend primitives only; host policy/approval/audit and product
  command routing are not part of this slice. Windows rename/delete, Android
  write-back, and product storage routing remain later work.

### H3.3 Android real workspace adapter

Status: in progress.

Add device/emulator execution and a SAF/ContentResolver or broker-backed workspace adapter. Keep raw Android storage paths out of portable requests.

Acceptance:

- arm64 device/emulator executes the portable command suite;
- document bytes can be read through a virtual path;
- permissions, cancellation, lifecycle, and disclosure tests pass;
- AAR packaging remains reproducible.

Evidence (2026-08-26):

- `MspAndroidContentResolverWorkspaceSource` now owns an authorized SAF tree
  `content://` URI, resolves child documents through `DocumentsContract`, and
  performs bounded binary reads. `MspAndroidWorkspaceProjection` projects the
  result into the existing Rust in-memory `/workspace/...` namespace; Android
  URI authorities and document IDs never enter native requests, diagnostics,
  logs, or audit records. Path validation rejects traversal, host separators,
  control characters, hidden `.msp`, and ambiguous duplicate names.
- Cancellation is connected to a cooperative lifecycle token and Android
  `CancellationSignal`; active operations are cancelled on close and calls
  after close fail as `Closed`. Provider, permission, not-found, limit, and
  native failures are reduced to typed path-free categories.
- Focused JVM tests passed for binary bytes, outside-mount rejection,
  cancellation, close, native workspace immutability, and disclosure rules.
  `compileDebugAndroidTestKotlin` passed after wiring the test manifest and
  provider class into the relocated androidTest source set.
- On 2026-08-26, API 35 x86_64 emulator
  `reados-api35-x86_64(AVD) - 15` ran
  `:msp-command-runtime-ffi:connectedDebugAndroidTest` successfully: 3 tests
  passed. The real `ContentResolver` test provider projected binary
  `/workspace/doc.bin` and Rust `cat` returned `00 ff 01 00`; cancellation,
  close, and host-disclosure boundaries also passed. The test-only provider is
  protected with `MANAGE_DOCUMENTS` and is registered in the merged
  androidTest manifest, so this is provider-boundary evidence rather than a
  direct Kotlin fake source.
- The arm64-v8a release AAR remains reproducible through the existing real
  `aarch64-linux-android` cdylib, 13-export check, Kotlin unit tests, and
  `verifyReleaseAar` gate. The x86_64 Rust artifact is used only for emulator
  instrumentation.

Limitations / remaining acceptance gap:

- This closes the Android SAF/ContentResolver projection and lifecycle slice,
  but H3.3 is not `done` until the portable command suite and this workspace
  path run on arm64 hardware or an arm64 emulator. The local x86_64 Windows
  QEMU setup rejects arm64 AVDs (`CPU Architecture 'arm64' is not supported`),
  so no arm64 runtime claim is made.
- The adapter is read-only and projection-based: SAF write-back, directory
  mutation, persistent permission/revocation flows, broker replacement, and
  host policy/approval/audit/product command routing remain outside this
  slice.

### H4.1 Common verified runtime provider contract

Status: done.

Unify bundle identity, command profiles, argv/environment/cwd policy, network, workspace projection, limits, cancellation, cleanup, and provenance into a host-neutral provider contract.

Acceptance:

- no host `PATH` or arbitrary shell-string fallback;
- launch-time identity revalidation;
- blocked states remain truthful;
- provider contract is independent of Windows, Linux, Android, Git, Python, Node, and Termux implementations.

Evidence (2026-08-26):

- `src/ReadOS.Msp.Hosting/Native/MspVerifiedRuntimeProviderContracts.cs`
  defines the host-neutral provider registration, command-profile, launch
  request/plan, bounded-limit, workspace-mode, lifecycle, provenance, and
  launch-identity observation contracts. It has no process, filesystem,
  network, PATH, platform, policy, approval, or audit dependency.
- Registration first evaluates verified, platform-neutral bundle evidence,
  then fail-closes missing/unverified identity, RID/architecture drift, unsafe or shell
  entry points, missing executable SHA-256, unspecified workspace/lifecycle
  policy, invalid limits, and missing license/NOTICE identifiers. A blocked
  registration contains no provider contract.
- Launch-plan validation accepts only a registered profile id, explicit argv,
  virtual cwd, and bounded environment values. It rejects shell wrappers,
  DOS/UNC/POSIX/file-URL host paths, PATH/loader environment keys, control/NUL
  values, and argument/environment overages. It never exposes a host path or
  shell command field.
- `RevalidateIdentity` compares fresh verification state, runtime, bundle id,
  manifest digest, RID, architecture, and executable digest immediately before a
  backend launch. Any drift returns the stable path-free
  `msp.provider.plan.identity_changed` blocked result.
- Focused `MspVerifiedRuntimeProviderContractTests` passed 13 tests on
  2026-08-26, covering ready registration, blocked capability, profile and
  shell rules, host-path/environment rejection, bounds, mandatory
  cancellation/process cleanup, provenance, truthful non-echoing blocked
  results, and bundle/executable identity revalidation.
- Proportionate verification also passed on the Windows checkout: Hosting
  tests `421 passed, 6 skipped`, MSP tests `69 passed`, App tests `425 passed`,
  `dotnet build ReadOS.sln --no-restore` completed with zero warnings/errors,
  the capability-manifest verifier passed (31 capabilities), and
  `git diff --check` reported no whitespace errors. The six Hosting skips are
  pre-existing release-DLL tests gated by absent explicit native-DLL input;
  they are not H4.1 provider-launch evidence.

Limitations:

- This closes the host-neutral contract gap only. The separate H4.2 Git slice
  below provides the first real Windows/Linux provider evidence; Python, Node,
  Toybox, BusyBox, and Termux remain unsupported. Host policy, approval, and
  product audit are still not implemented by the provider crates.

### H4.2 First external runtime slice

Status: done.

Implemented the first verified external runtime vertical slice: a read-only
Git provider with one fixed profile catalog on Windows and Linux. The model
selects `status`, `files`, `head`, or bounded `log` and a virtual
`/workspace/...` cwd; it cannot provide arbitrary Git argv, shell text, host
paths, hooks, credentials, remotes, or network options. Host policy,
approval, provider selection, and product audit remain above the provider.

Acceptance:

- same profile and result contract on both platforms;
- repository escape, hooks, config injection, network, child-process, cancellation, and cleanup tests;
- bundle provenance and license evidence in packages.

Evidence (2026-08-26):

- `native/msp-process-backend` owns host executable binding and SHA-256
  revalidation, virtual-cwd-to-trusted-root mapping, cleared environment,
  bounded stdout/stderr, timeout/cancellation, and process-tree cleanup
  (Windows Job Object; Unix process group). `native/msp-git-provider` owns the
  fixed Git profiles and isolated Git configuration (`GIT_CONFIG_NOSYSTEM=1`,
  empty credential helper, disabled hooks, prompts, optional locks, and
  network protocols).
- Windows evidence passed with Git `2.54.0.windows.1`, executable
  `C:\Program Files\Git\cmd\git.exe`, SHA-256
  `81ef35ae005ca9318018d18e3327578ce939fb99feaad6b2d7c8ab15f3de8db5`.
  `cargo test -p msp-process-backend -p msp-git-provider --locked` passed
  11 tests (6 backend + 5 provider), including cancellation, timeout, and
  the real Windows directory-symlink escape test; Clippy and the Windows
  target check passed.
- Linux evidence was run in the local WSL checkout (not hosted CI) with Git
  `2.55.0`, `/usr/bin/git`, SHA-256
  `93473c28694fd72bd889364107cd2770514de59780885a6a4aafca4d602e30ad`.
  The same 11 Rust tests and Clippy passed, including cancellation and the
  real Linux symlink escape test.
- `cargo package --list --allow-dirty --locked` passed for both crates, and
  `scripts/verify-msp-crate-packages.ps1` now checks their README, Apache
  license, NOTICE, profile manifest, and provenance files.

Limitations:

- Git is host-bound in this slice: the target Host supplies the executable and
  digest; the provider crates do not redistribute Git. This is provider/backend
  acceptance evidence, not packaged Git distribution or product command-route
  adoption.
- WSL is local non-Windows evidence, not a hosted Linux CI result. Android and
  Termux remain H4.3 work; unrestricted process/PTY support and full upstream
  Git parity are intentionally out of scope.

### H4.3 Android tool provider experiment

Status: in progress.

Decision and prototype are now present: start with an app-owned Toybox
provider, keep BusyBox as a later comparison, and reject Termux as an implicit
dependency. `MspAndroidToolProvider` is a platform-layer provider with fixed
`pwd`, `cat`, and `ls` profiles over an app-private projected workspace. It
  accepts only virtual cwd/path values, binds a digest-verified `toybox`
  executable supplied by the Host, clears the environment, bounds
output/time, terminates on cancellation/expiry, and redacts bound paths.

Acceptance:

- written decision covering size, licensing, update model, command coverage, sandbox, storage, network, process lifecycle, and user installation requirements;
- argv-only prototype over a projected temporary workspace;
- no unrestricted Termux shell session.

Evidence (2026-08-26):

- [ANDROID_TOOL_PROVIDER_DECISION.md](ANDROID_TOOL_PROVIDER_DECISION.md)
  records the Toybox-first choice, BusyBox/Termux trade-offs, licensing,
  update, sandbox, storage, network, lifecycle, and installation constraints.
- `native/msp-command-runtime-ffi/android/MspAndroidToolProvider.kt` plus the
  provider manifest/provenance implement the fixed-profile argv-only contract.
  JVM tests cover fixed argv, virtual escape/host syntax, digest drift,
  Termux rejection, pre-cancellation, and path-free failures. The Android project compiled
  `assembleDebugAndroidTest` for the x86_64 test ABI and the existing Gradle
  unit test task passed with the real Rust artifact and ABI export check.
- `MspAndroidToolProviderFactory` now defines the app-owned bundle boundary:
  metadata is loaded from fixed AAR assets, the executable is copied below the
  app-private `noBackupFilesDir`, the manifest/provenance/NOTICE and expected
  executable digest are checked, and missing or drifting assets fail closed.
  The AAR carries metadata only; no Toybox executable is redistributed.
- Hosting now exposes `MspVerifiedRuntimeProviderCatalog` and the
  `MspAndroidToyboxProviderContract` fixed `pwd`/`cat`/`ls` argv prefixes. The
  catalog registers verified evidence and creates virtual launch plans only;
  it does not execute processes or own policy, approval, or audit.
- Three focused Hosting tests passed for Toybox registration, fixed-prefix
  launch planning, virtual-path rejection, duplicate registration, and unknown
  provider handling.
- Android instrumentation tests are added for `/system/bin/toybox` as a
  `SystemToyboxOracle`, reading binary data from an app-private projected
  workspace and checking `pwd` path redaction. This is a test target, not a
  bundled Toybox distribution claim.
- On 2026-08-26, `reados-api35-x86_64(AVD) - 15` ran
  `:msp-command-runtime-ffi:connectedDebugAndroidTest` successfully: all 7
  instrumentation tests passed, including the new app-owned asset fail-closed
  case, system Toybox oracle reads, projected workspace binary reads,
  cancellation, and disclosure checks.

Limitations:

- The connected run is x86_64 emulator evidence only. Arm64 device/emulator
  execution remains open, and the app-owned Toybox executable is still absent.
- `AppOwnedBundle` is not enabled because ReadOS does not yet redistribute a
  Toybox artifact with pinned license/NOTICE and update evidence. BusyBox and
  Termux are not supported, and product Host policy/approval/audit routing is
  not yet connected to this provider.

### H5.1 Modular product routing

Status: in progress.

Stabilize packaged loading and move `pwd`/`echo`, then `cat`/`ls`, through the modular runtime as their workspace dependencies become available.

Acceptance:

- product policy, approval, events, terminal result, and exactly-once audit remain managed;
- managed-vs-modular differential passes;
- package smoke reports modular ABI/capabilities;
- App services contain no raw FFI ownership.

### H5.2 MSPChatUI browser runtime host

Status: in progress.

The product-owned `src/ReadOS.Web/MSPChatUI` Web renderer is now connected to
a development-only loopback Rust host. `native/msp-web-host` serves the browser
assets and exposes a narrow JSON API backed by the modular
`msp-command-runtime` and the `reados-portable-msp-v1` command pack. The
browser sends explicit chat or structured virtual-command turns; the Host
keeps provider credentials private and returns the canonical
`msp.chat-ui.timeline.v1` model already consumed by the Default renderer.

Acceptance slice completed:

- `/` now opens a responsive product-facing Web workbench instead of the
  single-command diagnostic page;
- the workbench provides a Codex-style session sidebar, conversation header,
  empty-state suggestions, accumulated rich timeline, multiline composer,
  virtual cwd control, runtime inspector, dark theme, and mobile navigation;
- the composer has explicit AI conversation and MSP command modes. The Host
  reads optional OpenAI-compatible provider configuration from environment
  variables and never sends the credential to the browser; only MSP mode can
  execute commands;
- Host-owned process-local session APIs create, list, load, accumulate, and
  delete conversations without exposing browser storage as the source of
  truth;
- one real `pwd` request reaches the Rust planner/registry/backend and renders
  through the existing Web UI;
- shell syntax (`pwd | cat`) is rejected by the portable planner;
- binary-safe stdout/stderr and stable exit/diagnostic fields are returned;
- each request response carries exactly one terminal audit evidence record;
- static asset serving is rooted to an explicit package directory and rejects
  traversal, host separators, and files outside the canonical root;
- the listener defaults to `127.0.0.1` and does not expose a remote service.

Evidence (2026-09-02):

- The runnable Web package was copied out of the ignored upstream checkout to
  `src/ReadOS.Web/MSPChatUI`; the Rust Host default no longer depends on
  `MSP/`, and the product package has its own ignore rules for caches,
  references, and build output.
- `cargo test -p msp-web-host --offline`: 8/8 passed, including local mock
  provider projection and the unconfigured-provider rejection path;
- `npm.cmd run check:static` and `npm.cmd run check:hygiene` pass from the
  product Web package;
- `npm.cmd run check:package` passes with an isolated npm cache (231 package
  files, 17 required entries); the local global npm cache remains EPERM;
- the tests cover binary base64 round-trip, portable runtime success, and
  shell-form rejection;
- `docs/MSP_WEB_HOST.md` and `src/ReadOS.Web/MSPChatUI/Hosts/Web/reados-runtime.html` document the
  browser-to-runtime boundary.
- A live loopback smoke on `127.0.0.1:8787` returned `health=ready`, runtime
  profile `reados-portable-msp-v1`, `pwd` exit code `0`, timeline schema
  `msp.chat-ui.timeline.v1`, audit count `1`, and HTTP `200` for the runtime
  page. The listener was stopped after the smoke.
- A workbench live smoke returned HTTP `200` for `/`, its CSS, and its JS;
  created one server-side session, executed `pwd` and `ls /workspace`, loaded
  four accumulated timeline messages at revision `2`, and observed one
  terminal audit record on the second turn. Headless Edge screenshots verified
  the 1440x960 desktop shell, rendered command timeline, and 390x844 responsive
  layout; screenshots are generated evidence under `artifacts/`.
- Real-browser console evidence also exposed a loader boundary bug: three
  manifest entries are native WebView function-body snippets, not standalone
  scripts. `default-web-loader.js` now excludes the host-command invocation,
  bootstrap-probe, and selection-repair snippets from `<script src>` loading
  while retaining the actual browser selection context-menu script. This
  removes top-level `return` and unresolved-placeholder errors from the Web
  host without changing native WebView contracts.

Limitations:

- this is a development/browser Host, not authenticated multi-user serving;
- sessions are process-local and reset when `msp-web-host` exits;
- AI chat is non-streaming and has no tool-call orchestration yet; provider
  answers cannot execute commands, while MSP mode remains a separately
  constrained and audited path;
- the workspace is an in-memory fixture and is not yet the product session or
  artifact store;
- durable product policy, approval, session projection, and audit persistence
  still belong to the consuming ReadOS Host;
- external providers (Git/Toybox/Termux) are not reachable through this API;
  they require their own verified provider registration and Host approval;
- no arbitrary shell, host path, PATH lookup, or remote bind is supported.

### H6.1 Close the flagship packaged workflow

Status: in progress.

Run the document evidence/synthesis workflow through a real provider and a real packaged-process restart, then restore lineage in the visible workbench.

Acceptance:

- no credentials or provider bodies in logs, transcripts, audit, artifacts, or package evidence;
- cancellation, denial, invalid page, provider failure, restart, and retry branches remain intact;
- operator runbook and current screenshots/recording match the shipped UI.

### H7.1 Legacy retirement readiness

Status: planned.

Create a generated retirement report from the migration matrix and package dependency graph.

Acceptance:

- report shows zero required legacy capabilities, product references, DLL variables, package entries, and CI jobs;
- first legacy-free package has rollback instructions and full release evidence.

## Deferred Until Dependencies Exist

- General external process/PTY support before H4.1 provider policy is complete.
- Unrestricted Termux integration.
- Complete POSIX/GNU command parity.
- Additional Android ABIs before arm64 device evidence.
- Moving PDF, chat, workflow, credentials, artifacts, or UI behavior into Rust.
- Removing legacy source before H7 gates pass.

## Next Item

Next priority: continue H4.3 with a reviewed app-owned Toybox bundle (binary,
license/NOTICE, expected digest, and update provenance) and then wire the
registered provider through Android Host policy/approval/audit. H3.3 arm64 Android runtime evidence remains blocked by the local
x86_64-only emulator/QEMU environment, so it is not treated as an unblocked
dependency. H4.1's host-neutral contract and H4.2's Windows/Linux Git provider
slice are now closed for their current contracts; H4.3's contract/prototype
is present but not yet a supported provider. H3.1 Windows
retained-handle and H3.2 Linux openat2 workspace slices are closed for their
current contracts; Android product write-back and provider adoption remain
separate H3/H4 work, while product command routing remains H5 work.

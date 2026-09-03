# ReadOS Hybrid MSP Development Plan

## Purpose

This plan moves ReadOS from two overlapping native-runtime generations to one hybrid MSP architecture:

- the product host owns policy, approval, audit, sessions, artifacts, and domain services;
- the portable Rust runtime owns deterministic command semantics;
- platform backends own safe filesystem, process, PTY, storage, and sandbox capabilities;
- small virtual-workspace builtins are implemented in-process;
- mature complex tools are integrated through verified external runtime providers.

The authoritative architecture is [docs/MSP_HYBRID_ARCHITECTURE.md](docs/MSP_HYBRID_ARCHITECTURE.md). Active item status is maintained in [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md).

## Current State

The repository already contains:

- a functional WinUI ReadOS workbench and document vertical;
- managed command, policy, audit, session, transcript, artifact, and virtual-workspace layers;
- a legacy Windows-focused `native/msp-core` with substantial WorkspaceFS, shell, stream, session, process, PTY, and ABI functionality;
- a modular portable Rust workspace containing kernel, backend, shell, command-pack, command-runtime, protocol, adapter, and FFI crates;
- a Linux `openat2` workspace backend;
- an Android arm64 Kotlin/JNI/AAR binding for the portable command runtime;
- optional product adoption of the modular command-runtime FFI for a bounded command slice.

The central problem is no longer missing code. It is convergence: remove duplicated ownership, finish safe platform backends, prove real product adoption, and retire the legacy runtime.

## Delivery Principles

1. Close product acceptance and convergence gaps before adding more commands.
2. Do not add new generic runtime features to `native/msp-core` except security or migration fixes.
3. Do not implement complete replacements for GNU Coreutils, Termux, Git, Python, or Node.
4. Add portable builtins only when virtual-workspace semantics or deterministic safety require an in-process implementation.
5. Add complex tools through verified external runtime providers.
6. Keep host policy and terminal product audit authoritative.
7. Make capability claims only from target-platform runtime evidence.
8. Deliver vertical slices with implementation, tests, packaging, documentation, and explicit remaining limitations.

## Phase H0 — Documentation and Governance Reset

Status: done.

Objective: establish one architecture, one plan, one active tracker, and one short continuous-development prompt.

Deliverables:

- authoritative hybrid architecture specification;
- rewritten product goal and development plan;
- concise current tracker without historical T1-T95 execution logs;
- removal of superseded architecture, SDK migration, prompt-injection loop, and long autonomous prompt documents;
- README and repository guidance updated to point to the new authority chain.

Acceptance:

- no repository link points to a removed planning document;
- current component documents remain available as implementation evidence;
- architecture ownership statements are consistent across README, AGENTS, plan, and tracker.

## Phase H1 — Legacy Capability Inventory and Freeze

Status: in progress.

Objective: make the two runtime generations explicit and prevent further divergence.

Deliverables:

1. Add `docs/MSP_RUNTIME_MIGRATION_MATRIX.md` with one row per legacy capability:
   - parser/AST;
   - expansion;
   - command registry and builtins;
   - byte streams;
   - workspace path and mixed mounts;
   - Windows retained-handle WorkspaceFS;
   - writes and recoverable trash;
   - pipeline/redirection;
   - sessions;
   - process/PTY;
   - sanitization;
   - verified bundles and launch plans;
   - ABI/FFI and managed adapter;
   - product routing and packaging.
2. Classify each row as `port`, `adapt`, `defer`, or `delete`.
3. Name the modular destination crate and acceptance test for every `port` or `adapt` row.
4. Add a repository rule that rejects new product references to raw legacy FFI surfaces.
5. Limit legacy changes to security, release, differential, and migration-enabling fixes.

Acceptance:

- every legacy source module has a disposition;
- no capability is claimed by both implementations without a named migration owner;
- current packages and product routing are documented truthfully.

## Phase H2 — Portable Runtime Contract Convergence

Status: in progress.

Objective: make the modular Rust workspace the single implementation of portable command semantics.

Work packages:

### H2.1 Stable command request and result

- freeze versioned request/result/error DTOs;
- preserve binary stdin/stdout/stderr;
- define stable exit codes, diagnostic categories, truncation, cancellation observation, and execution summaries;
- keep actor, policy, approval, audit, provider, and host paths outside the runtime request.

### H2.2 Planner and execution boundary

- keep `prepare` separate from `execute`;
- expose only bounded metadata for policy decisions;
- prevent registry/context substitution between prepare and execute;
- support cooperative cancellation and truthful interruption reporting.

### H2.3 Builtin command profile

- retain a small deterministic set: `pwd`, `echo`, `printf`, `cat`, `ls`, `find`, `du`, `head`, `tail`, `wc`, `grep`, and selected `sed` behavior;
- document supported options and deviations;
- reject host paths, shell escapes, process actions, and unsupported options with stable diagnostics;
- add commands only when required by a product workflow or conformance gate.

### H2.4 ABI convergence

- select one supported modular C ABI;
- align .NET and Android ownership, limits, result projection, and ABI checks;
- add release export and allocator verification;
- keep the legacy ABI only as a temporary compatibility input.

Acceptance:

- the modular runtime executes the selected portable command profile identically on Windows, Linux, and Android test hosts;
- no portable crate accesses host filesystem, process, environment, `PATH`, policy, audit, or product services;
- public adapters consume the same versioned ABI and test vectors.

## Phase H3 — Real Platform Workspace Backends

Status: in progress.

Objective: give each target platform a real, safe workspace implementation behind the same `WorkspaceBackend` contract.

### H3.1 Windows

- port/adapt retained-root and final-handle containment from the legacy core;
- support bounded stat/list/range read and approved write primitives;
- reject drive/UNC/device/ADS/reparse escape forms;
- hide `.msp` after final-handle resolution;
- expose the implementation through `msp-backend-windows`, not through legacy core types.

### H3.2 Linux

- preserve owned root descriptor and `openat2` confinement;
- complete read/write/rename/delete behavior required by the common contract;
- add supported-kernel integration evidence and unsupported-kernel fail-closed evidence;
- do not add unsafe canonicalize-and-reopen fallback.

### H3.3 Android

- retain the in-memory FFI workspace for deterministic SDK use;
- design a `ContentResolver`/SAF or broker-backed workspace adapter;
- use opaque content identifiers above the Android platform layer;
- avoid exposing raw shared-storage paths to the portable runtime;
- add arm64 device or emulator instrumentation evidence.

### H3.4 Composite workspace

- support longest-prefix virtual mounts and path rebasing;
- define capability composition for read, write, usage, and mutation;
- prove callback lifetime, disposal, concurrency, and cancellation behavior across .NET and JNI hosts.

Acceptance:

- Windows and Linux pass the same workspace contract suite against real host directories;
- Android passes the same suite through an actual device/emulator adapter;
- host paths never appear in results, diagnostics, audit examples, or model-visible output.

## Phase H4 — Verified External Runtime Providers

Status: in progress.

Objective: use mature tools without granting arbitrary shell access or reimplementing them.

### H4.1 Common provider contract

Define:

- runtime family and immutable bundle identity;
- manifest digest and executable identity;
- supported command profiles;
- argv/environment/cwd rules;
- workspace projection or broker rules;
- network policy;
- timeout, output, memory, process-count, and file-count limits;
- cancellation and process-tree cleanup;
- version, license, NOTICE, and provenance metadata.

### H4.2 Windows process provider

- port/adapt verified bundle launch and Job Object cleanup;
- provide pipe mode first, then ConPTY only for commands that need terminal semantics;
- revalidate executable identity at launch;
- sanitize host paths in UTF-8 and UTF-16LE output.

The first H4.2 vertical slice is implemented in the shared
`native/msp-process-backend` and `native/msp-git-provider` crates. Windows
uses a Job Object and the host-bound Git executable; Linux uses the same
provider contract with a Unix process group. The remaining H4 work is
provider integration into the managed host, packaged executable policy, and
additional hardening such as namespaces/seccomp/cgroups where available.

### H4.2.1 Linux confinement hardening

The shared H4.2 process backend already launches only verified executables,
uses a cleared minimal environment, bounds pipe output, and kills the complete
Unix process group on cancellation or expiry. Extend it with namespace,
seccomp, cgroup, and file-descriptor controls where the target distribution
provides reliable evidence; unsupported controls must remain explicit rather
than silently falling back.

### H4.3 Android provider

The first experiment is now present in the Android platform layer:

- select an app-owned Toybox bundle as the production direction;
- use fixed `pwd`/`cat`/`ls` profiles over an app-private projected workspace;
- require exact executable digest, profile manifest, license, and NOTICE evidence;
- treat Termux as an explicit optional source that fails closed in this slice;
- prohibit unrestricted Termux shell sessions and implicit shared-storage access.

The current `/system/bin/toybox` target is instrumentation-only oracle
evidence. App-owned artifact packaging, device execution, and managed Host
policy/approval/audit adoption remain open.

The Host-side provider boundary is now implemented: the verified provider
catalog can register the Toybox contract and create only fixed `pwd`/`cat`/`ls`
launch plans with exact argv prefixes and virtual-workspace suffixes. The
Android binding consumes the same metadata but still fails closed until an
app-owned Toybox asset and expected digest are supplied.

### H4.4 Initial tool families

Adopt in this order (Git is the completed first provider slice):

1. Git read-only inspection profile;
2. Python bounded script/profile execution;
3. Node bounded script/profile execution;
4. selected platform utilities only when they reduce builtin duplication.

Acceptance:

- at least Windows and one non-Windows platform execute the same registered external command profile;
- no provider uses host `PATH` or shell command strings;
- cancellation, cleanup, network policy, identity drift, output bounds, and path sanitization have negative tests;
- packages include required licenses and provenance.

## Phase H5 — Product Adoption of the Modular Runtime

Status: in progress.

Objective: make ReadOS consume the modular runtime through one supported Hosting adapter.

Work packages:

1. Stabilize `MspCommandRuntimeFfiAdapter` and packaged loader behavior.
2. Route `pwd` and `echo` through the modular runtime.
3. Route `cat` and `ls` after the app callback/composite workspace gate passes.
4. Adopt additional portable builtins only when they are used by a real product workflow.
5. Keep PDF, chat, workflow, artifact, credential, persistence, and WinUI commands managed.
6. Preserve managed policy, approval, streaming projection, terminal result, and exactly-once audit authority.
7. Add managed-vs-modular differential tests during migration.
8. Expose runtime capability and version information in package smoke evidence.

Acceptance:

- the supported product package loads the modular runtime from the package directory only;
- failure to load is explicit and follows the documented availability policy;
- adopted commands have no behavior, audit, policy, or path-disclosure regression;
- no App service owns P/Invoke or native handles.

## Phase H6 — Product-Level Acceptance

Status: in progress.

Objective: prove that runtime work produces a reliable user-visible product.

Flagship scenario:

```text
import PDF
→ select outline section
→ extract evidence
→ approve artifact write
→ synthesize through a real provider
→ persist session/transcript/artifact/manifest
→ restart the packaged app process
→ restore state and navigate lineage to the original page
```

Required branches:

- denial without partial output;
- cancellation without partial artifact;
- invalid page/outline recovery;
- provider failure with no secret or response-body disclosure;
- retry after restart;
- missing native capability with truthful product behavior;
- package cleanup and private-data exclusion.

Acceptance:

- service automation, packaged-process automation, and a minimal visible WinUI smoke all pass;
- at least one real provider/network run is recorded without committing credentials or response bodies;
- operator runbook and screenshots/recording match current UI behavior.

## Phase H7 — Legacy Runtime Retirement

Status: planned.

Objective: remove `native/msp-core`, `native/msp-ffi`, the legacy internal ABI, and duplicate product routing.

Retirement gates:

- all `port`/`adapt` rows in the migration matrix are complete;
- all adopted product commands use the modular runtime;
- Windows workspace and process requirements are satisfied by modular backends;
- managed/native differential tests have no unexplained drift;
- release packages and smoke tests pass without the legacy DLL;
- licenses, notices, exports, scripts, workflows, and docs reference only supported artifacts;
- rollback instructions exist for the first legacy-free release.

Removal work:

- delete legacy crates and target-specific build scripts;
- delete legacy Hosting adapter code and tests;
- delete legacy DLL variables and package gates;
- remove obsolete compatibility text while retaining historical release evidence where legally or operationally required.

## Phase H8 — Multi-Platform SDK and Product Expansion

Status: planned.

Objective: turn portable runtime evidence into supported SDK profiles.

Deliverables:

- versioned Windows, Linux, and Android capability profiles;
- generated C header plus .NET/Kotlin examples;
- Android device-tested AAR;
- Linux native package/sample host;
- conformance reports that distinguish portable semantics from platform capability coverage;
- semantic-versioning and compatibility policy;
- release provenance and SBOM for bundled external runtimes.

This phase does not require the WinUI product itself to become cross-platform.

## Execution Order

The required order is:

```text
H1 inventory/freeze
→ H2 portable contracts
→ H3 real workspace backends
→ H4 verified external runtimes
→ H5 product adoption
→ H6 product acceptance
→ H7 legacy removal
→ H8 supported multi-platform SDK profiles
```

H5 and H6 may advance in small slices while H2-H4 are underway, but legacy removal cannot begin until its gates are satisfied.

## Work Item Template

Every tracker item must state:

- objective;
- owning layer/crate/project;
- dependency and capability assumptions;
- security invariants;
- implementation deliverables;
- focused tests;
- target-platform evidence;
- package/documentation impact;
- explicit non-goals;
- completion evidence and remaining limitations.

## Verification Strategy

Use the narrowest relevant checks first, then broaden in proportion to risk:

- managed boundary changes: affected xUnit project, then `dotnet build ReadOS.sln`;
- portable Rust changes: package tests, Clippy, formatting, cross-target build;
- Windows backend/process changes: Windows integration tests plus package smoke;
- Linux backend changes: Ubuntu runtime integration tests, not cross-compilation alone;
- Android changes: Rust cross-build, JVM tests, AAR verification, then device/emulator instrumentation;
- ABI changes: export, ownership, malformed-input, real-library, and package tests;
- product routing changes: managed/native differential plus flagship workflow branch;
- external runtime changes: identity drift, network, cleanup, cancellation, output, path disclosure, provenance, and package gates.

## Release Rule

A release may claim only the capabilities proven by that release's target-platform gates. Unsupported and blocked capabilities remain visible as such; they do not silently fall back to arbitrary host behavior.

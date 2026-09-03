# ReadOS

ReadOS is an MSP-first Windows workbench and reference vertical host for agent-native applications. It combines a real document/PDF product with a controlled command runtime, virtual workspace, policy and approval, terminal audit, sessions, transcripts, and durable artifacts.

The current direction is a hybrid MSP architecture:

- the product host owns authority and domain behavior;
- a modular Rust runtime owns portable deterministic command semantics;
- platform backends own safe workspace, process, PTY, storage, and sandbox integration;
- small virtual-workspace builtins run in-process;
- complex mature tools run through verified external runtime providers rather than being rewritten or exposed through arbitrary shell access.

Read [PRODUCT_GOAL.md](PRODUCT_GOAL.md), [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md), and [docs/MSP_HYBRID_ARCHITECTURE.md](docs/MSP_HYBRID_ARCHITECTURE.md) before changing architecture or runtime ownership. Active work is tracked in [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md).

## Repository Structure

- `src/ReadOS.App`: WinUI workbench, PDF/document services, chat/provider integration, workspace persistence, app command packs, operator policy, and UI state.
- `src/ReadOS.Msp`: managed runtime contracts, parser, registry, policy, audit, results, and virtual workspace abstraction.
- `src/ReadOS.Msp.Hosting`: host-neutral command host, sessions, artifacts, approval grants, cancellation, runtime composition, and native adapters.
- `native/msp-kernel`, `native/msp-backend`, `native/msp-shell-*`, `native/msp-command-pack`, `native/msp-command-runtime`, `native/msp-command-runtime-ffi`: target modular portable runtime.
- `native/msp-backend-linux`, `native/msp-backend-windows*`: platform backend work.
- `native/msp-command-runtime-ffi/android`: Android arm64 Kotlin/JNI/AAR binding.
- `native/msp-core`, `native/msp-ffi`: retained legacy runtime and compatibility/release inputs pending migration and retirement.
- `tests`: managed Core, Hosting, and App xUnit suites.
- `conformance`: versioned upstream/reference capability and fixture evidence.
- `artifacts`: generated builds, smoke evidence, and release packages.

## Current Product Capability

The Windows workbench currently includes:

- local project/workspace persistence;
- PDF, Markdown, and text import;
- PDF rendering, thumbnails, text extraction, search, outlines, and page labels;
- document conversations, evidence attachments, and OpenAI-compatible chat with offline fallback;
- domain commands for workspace, library, PDF, document metadata, attachments, chat, artifacts, and workflows;
- evidence extraction, review, synthesis, refinement, provenance, and lineage workflows;
- typed command timeline, approvals, cancellation, diagnostics, artifacts, and Runtime Drawer supervision;
- provider credentials protected outside workspace JSON/exports with Windows DPAPI.

The virtual workspace exposes product state through paths such as:

```text
/projects
/library
/documents
/artifacts
/sessions
/transcripts
/settings.json
```

The product host remains authoritative for policy, approval, terminal results, exactly-once audit, sessions, artifacts, PDF/chat/provider behavior, and persistence.

## Native Runtime Status

The modular Rust workspace already provides portable contracts for virtual paths, capabilities, shell parsing, expansion, command registration, a bounded builtin command pack, stateless command execution, protocol projection, FFI, Linux workspace binding, and Android packaging.

Capability coverage is intentionally uneven:

- Linux has a real `openat2`-anchored host workspace backend; process and PTY remain unsupported.
- Android packages the portable runtime in an arm64 AAR; the canonical portable
  fixture runs through a test-only x86_64 emulator variant, while device
  storage and process providers remain future work.
- The modular Windows host workspace now has a retained-handle stat/list/range-read and bounded whole-file write slice; process/PTY and broader write/trash behavior remain separate work.
- `native/msp-core` contains working Windows functionality used as compatibility and migration evidence, but it is not the permanent architecture.
- Modular product adoption is bounded and must preserve managed host policy/audit authority.

Do not infer platform support from compilation, stubs, feature flags, or package assembly alone. See [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md) for current claimed evidence and gaps.

## Build and Test

Build only:

```powershell
.\scripts\run.ps1 -BuildOnly
```

Build and launch:

```powershell
.\scripts\run.ps1
```

Build the full solution:

```powershell
dotnet build .\ReadOS.sln
```

Run managed tests:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
```

Run the full Windows MSP verification path:

```powershell
.\scripts\verify-msp.ps1
```

Run the modular Rust workspace:

```powershell
cargo test --workspace
```

Check the legacy-runtime migration boundary:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-msp-legacy-boundary.ps1
```

Create a Windows x64 package:

```powershell
.\scripts\package-windows.ps1 -StopExisting
```

## Architecture Rules

- Commands operate on the virtual workspace, not arbitrary host paths.
- Normalize paths before namespace membership checks.
- Mutating or external effects pass product policy and approval before execution.
- Every command attempt produces a terminal result and product audit record.
- Portable crates do not access host filesystem, process, environment, `PATH`, product services, policy, or audit.
- Platform backends provide capabilities but do not authorize them.
- App/domain code never owns raw FFI or native handles.
- Do not expose `bash -c`, `sh -c`, `cmd /c`, PowerShell command strings, unrestricted Termux sessions, or host `PATH` resolution to the model.
- Add portable builtins only for safe virtual-workspace semantics; integrate Git, Python, Node, Toybox, BusyBox, or Termux packages through verified runtime providers.
- Do not add new general-purpose features to legacy `native/msp-core` except security or migration-enabling work.

## Documentation

Authoritative documents:

- [PRODUCT_GOAL.md](PRODUCT_GOAL.md): product mission, scope, and non-goals.
- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md): dependency-ordered hybrid-runtime development plan.
- [docs/MSP_HYBRID_ARCHITECTURE.md](docs/MSP_HYBRID_ARCHITECTURE.md): layer ownership, command strategy, platform profiles, Termux rules, security invariants, and legacy convergence.
- [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md): active milestone and work-item status.
- [docs/GOAL_PROMPT.md](docs/GOAL_PROMPT.md): short prompt for continuous development.

Current component and evidence documents:

- [docs/MSP_KERNEL_FACADE.md](docs/MSP_KERNEL_FACADE.md)
- [docs/MSP_COMMAND_RUNTIME.md](docs/MSP_COMMAND_RUNTIME.md)
- [docs/MSP_COMMAND_PACK.md](docs/MSP_COMMAND_PACK.md)
- [docs/MSP_COMMAND_RUNTIME_FFI.md](docs/MSP_COMMAND_RUNTIME_FFI.md)
- [docs/MSP_BACKEND_LINUX.md](docs/MSP_BACKEND_LINUX.md)
- [docs/MSP_BACKEND_WINDOWS_CORE_BRIDGE.md](docs/MSP_BACKEND_WINDOWS_CORE_BRIDGE.md)
- [docs/ANDROID_RUNTIME_BINDING.md](docs/ANDROID_RUNTIME_BINDING.md)
- [docs/PORTABLE_RUST_CI.md](docs/PORTABLE_RUST_CI.md)
- [docs/MSP_NATIVE_RUNTIME_REGISTRATION.md](docs/MSP_NATIVE_RUNTIME_REGISTRATION.md)
- [docs/MSP_UPSTREAM_COMPATIBILITY_MATRIX.md](docs/MSP_UPSTREAM_COMPATIBILITY_MATRIX.md)
- [conformance/msp-upstream/windows-capability-manifest.json](conformance/msp-upstream/windows-capability-manifest.json)
- [docs/UI_UX_DESIGN.md](docs/UI_UX_DESIGN.md)
- [docs/OPERATOR_RUNBOOK.md](docs/OPERATOR_RUNBOOK.md)
- [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md)

The local `MSP/` reference checkout is an upstream/reference input only. It must not become a runtime dependency, Git payload, package payload, or source of unrecorded copied implementation code.

The product-owned browser UI lives under `src/ReadOS.Web/MSPChatUI`. The
loopback Rust browser host serves that directory by default; `MSP/` is not
required to run the Web UI.

Run `cargo run -p msp-web-host --offline` and open
`http://127.0.0.1:8787/` for the local session-based Web workbench.

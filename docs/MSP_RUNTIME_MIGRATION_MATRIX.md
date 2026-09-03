# MSP Runtime Migration Matrix

Updated: 2026-08-26.

This matrix is the inventory and ownership bridge between the retained legacy runtimes and the modular hybrid MSP architecture. It is intentionally explicit: no legacy capability may remain “temporarily” without a destination, owner, acceptance gate, and retirement dependency.

The target architecture is defined in [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). The matrix is verified by `scripts/verify-msp-runtime-migration-matrix.ps1`.

## Disposition vocabulary

- `port`: move the portable behavior into the modular Rust workspace with compatible semantics.
- `adapt`: keep platform or compatibility behavior, but expose it through a modular backend/provider/adapter boundary.
- `defer`: do not migrate until a named product or platform gate requires it; keep legacy evidence only.
- `delete`: remove after required evidence is preserved elsewhere; the legacy implementation is not a target.

## Authority rules

1. `native/msp-core` and `native/msp-ffi` are migration sources and compatibility inputs, not permanent product dependencies.
2. Portable parser, expansion, virtual path, command, stream, result, and limit semantics belong to the modular Rust workspace.
3. Windows/Linux/Android filesystem, process, PTY, storage, and runtime-bundle behavior belongs to platform adapters.
4. PDF, chat, provider, credential, workflow, artifact, session, approval, and product audit behavior belongs to the managed host.
5. No new product code may depend directly on a legacy FFI export.

## Inventory

| Legacy path pattern | Disposition | Modular/host destination | Owner | Acceptance gate | Retirement dependency |
| --- | --- | --- | --- | --- | --- |
| `native/msp-core/Cargo.toml` | delete | Modular workspace manifests | Runtime convergence | H7 legacy-free package | All legacy crates removed from build graph |
| `native/msp-core/Cargo.lock` | delete | Modular workspace lockfile | Runtime convergence | H7 reproducible package | No legacy dependency remains |
| `native/msp-core/.cargo/**/*.toml` | adapt | Modular target/build configuration | Runtime convergence | H2/H7 workspace build gate | Legacy standalone build configuration removed |
| `native/msp-core/include/msp_core.h` | adapt | Modular ABI header generation | FFI owner | H2 canonical ABI export check | No consumer includes legacy header |
| `native/msp-core/UPSTREAM_MSP.md` | delete | Compatibility matrix and provenance docs | Conformance owner | H1 provenance review | Required notices/evidence copied to current docs |
| `native/msp-core/src/abi_v2.rs` | adapt | `msp-command-runtime-ffi` | FFI owner | H2 ownership/limit/malformed-input suite | Product no longer loads legacy ABI |
| `native/msp-core/src/lib.rs` | adapt | Modular runtime module/export surface | Runtime convergence | H2 module and export review | Legacy crate no longer built |
| `native/msp-core/src/byte_stream.rs` | port | `msp-backend` + `msp-command-runtime` stream contracts | Runtime owner | H2 byte-stream contract suite | Modular streams cover all adopted routes |
| `native/msp-core/src/command_core.rs` | port | `msp-command-pack` registry/invocation contracts | Command-pack owner | H2 deterministic registry suite | No legacy dispatch path |
| `native/msp-core/src/contract.rs` | port | `msp-kernel`/`msp-protocol-windows` DTOs | Kernel owner | H2 versioned request/result vectors | No legacy DTO crosses adapter |
| `native/msp-core/src/output_sanitizer.rs` | adapt | Platform adapter output sanitization | Platform runtime owner | H4 UTF-8/UTF-16LE split-boundary tests | All process output uses provider boundary |
| `native/msp-core/src/pipeline.rs` | port | `msp-command-runtime` stream/pipeline execution | Runtime owner | H2 portable pipeline suite | Product routes use modular runtime |
| `native/msp-core/src/runtime.rs` | port | `msp-command-runtime` | Runtime owner | H2 prepare/execute differential | No duplicate runtime authority |
| `native/msp-core/src/sed.rs` | port | `msp-command-pack` safe subset | Command-pack owner | H2 documented subset fixtures | Legacy-only options are removed or deferred |
| `native/msp-core/src/shell.rs` | port | `msp-shell-language` | Parser owner | H2 parser boundary fixtures | No product parser depends on legacy module |
| `native/msp-core/src/session.rs` | adapt | Runtime sessions plus managed Host session projection | Runtime/Hosting owners | H4 exec/write_stdin lifecycle | Product session authority remains managed |
| `native/msp-core/src/workspace_path.rs` | port | `msp-backend::VirtualPath` | Backend owner | H2 path and disclosure suite | No legacy path type in public adapters |
| `native/msp-core/src/workspace_capabilities.rs` | port | `msp-backend::CapabilityReport` | Backend owner | H3 capability matrix | All platform claims use modular report |
| `native/msp-core/src/workspace_fs.rs` | adapt | `msp-backend-windows` retained-handle backend | Windows backend owner | H3 real Windows containment suite | Legacy WorkspaceFS no longer packaged |
| `native/msp-core/src/composite_workspace.rs` | adapt | Modular composite/mixed workspace backend | Backend owner | H3 mount routing/lifetime suite | No legacy composite type crosses FFI |
| `native/msp-core/src/workspace_callback.rs` | adapt | Modular callback workspace bridge | Adapter owner | H3 callback ownership/disposal suite | Product uses modular callback ABI |
| `native/msp-core/src/workspace_invoke.rs` | adapt | Modular workspace invoke operation | Adapter owner | H3 mixed-read differential suite | Legacy operation removed from package |
| `native/msp-core/src/workspace_trash.rs` | adapt | Platform backend recoverable trash | Backend/Host owners | H3 write/trash policy suite | Product policy/audit remains Host-owned |
| `native/msp-core/src/external_runner.rs` | adapt | Verified external runtime provider | Process backend owner | H4 identity/cleanup/cancel suite | No legacy launcher in release package |
| `native/msp-core/src/verified_bundle.rs` | adapt | Host-neutral verified-bundle contract | Provider owner | H4 bundle/provenance gate | One provider contract is authoritative |
| `native/msp-core/src/process.rs` | adapt | Windows/Linux process backend | Process backend owner | H4 process policy and kill-tree suite | Legacy process implementation retired |
| `native/msp-core/src/git.rs` | defer | H4 Git provider profile | External runtime owner | H4 first provider slice | No product claim before provider evidence |
| `native/msp-core/src/node_launch_plan.rs` | defer | H4 Node provider profile | External runtime owner | H4 Node identity/sandbox gate | No Node launch from legacy core |
| `native/msp-core/src/node_vfs.rs` | defer | H4 Node workspace broker | External runtime owner | H4 Node VFS gate | No legacy Node VFS package dependency |
| `native/msp-core/src/python_launch_plan.rs` | defer | H4 Python provider profile | External runtime owner | H4 Python identity/sandbox gate | No Python launch from legacy core |
| `native/msp-core/src/python_broker_protocol/**/*.rs` | defer | H4 Python broker protocol | External runtime owner | H4 authenticated bounded broker suite | Provider decision and platform evidence |
| `native/msp-core/src/chat_package.rs` | delete | Managed ReadOS chat/session/artifact persistence | App/Hosting owners | H6 product persistence acceptance | No chat state in generic runtime |
| `native/msp-core/src/chat_store.rs` | delete | Managed ReadOS workspace/session stores | App/Hosting owners | H6 restart and recovery workflow | No legacy chat store package dependency |
| `native/msp-core/src/bin/**/*.rs` | delete | Modular platform integration fixtures | Platform test owners | H3/H4 target integration fixtures | Legacy helper binaries not shipped |
| `native/msp-core/tests/**/*.rs` | adapt | Modular Rust and platform integration tests | Test owners | H2-H4 equivalent evidence | No legacy-only acceptance gate |
| `native/msp-ffi/Cargo.toml` | delete | `msp-command-runtime-ffi` manifest | FFI owner | H2 canonical ABI package | Public legacy crate retired |
| `native/msp-ffi/Cargo.lock` | delete | Modular workspace lockfiles | FFI owner | H7 package graph check | No legacy crate build |
| `native/msp-ffi/build.rs` | adapt | Modular FFI build/export verification | FFI owner | H2 export and symbol gate | Legacy build script removed |
| `native/msp-ffi/src/**/*.rs` | adapt | `msp-command-runtime-ffi` | FFI owner | H2 C ABI contract suite | Product and SDK use modular ABI |
| `native/msp-ffi/tests/**/*` | adapt | Modular FFI contract tests | FFI/test owners | H2 malformed-input/ownership suite | No unique legacy test behavior |
| `native/msp-ffi/include/**/*` | adapt | Modular generated/public header | FFI owner | H2 header consumer compile gate | Legacy header removed |
| `native/msp-ffi/exports/**/*` | adapt | Modular export definition | FFI owner | H2 exact export list | Legacy DLL exports removed |
| `native/msp-ffi/bindings/cpp/**/*` | defer | Future modular C++ SDK binding | SDK owner | H8 SDK profile gate | No legacy binding distribution |
| `native/msp-ffi/bindings/dotnet/**/*` | adapt | `ReadOS.Msp.Hosting.Native` adapter | Hosting owner | H2 .NET real-DLL suite | No raw legacy P/Invoke in App |
| `native/msp-ffi/bindings/node/**/*` | defer | Future H8 Node SDK binding | SDK owner | H8 SDK profile gate | No legacy Node binding distribution |
| `native/msp-ffi/release-metadata.json` | delete | Current release provenance/SBOM | Release owner | H7 package provenance gate | Legacy release artifact retired |

## Current gaps revealed by the matrix

- The modular Windows host-backed workspace backend has a real retained-handle stat/list/range-read and bounded whole-file write slice; product adoption, common cross-platform fixtures, and broader write/trash/process capabilities still rely on later gates.
- Process and PTY providers are not a portable-core feature. They require H4 platform/runtime-provider work.
- Git, Python, Node, and Termux are provider work, not commands to copy into `msp-command-pack`.
- Managed product chat/session/artifact behavior must not migrate into generic Rust.
- The legacy runtime remains in the release graph until H7 retirement gates pass.

## Completion rule

H1.1 is complete only when `scripts/verify-msp-runtime-migration-matrix.ps1` passes and every scoped legacy source/module file matches exactly one disposition row. The matrix does not claim that any migration is already complete; it makes the remaining work auditable.

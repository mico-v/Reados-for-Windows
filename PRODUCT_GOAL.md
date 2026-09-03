# ReadOS Product Goal

ReadOS is an MSP-first Windows workbench and reference vertical host for agent-native applications. It proves that an AI agent can operate real product state through a controlled command environment instead of arbitrary shell access or a loose collection of tool calls.

The document/PDF experience is the first vertical domain. It is valuable product functionality, but its larger purpose is to prove the reusable MSP host pattern end to end.

## Mission

Build a product-shaped MSP host where:

- application data is exposed through a stable virtual workspace;
- application capabilities are exposed as commands;
- mutations and external effects require explicit policy and approval;
- every attempt produces inspectable terminal evidence and audit;
- outputs become durable artifacts with provenance; and
- a human operator can supervise, cancel, retry, and review agent work.

## Product Thesis

```text
Data as virtual files.
Actions as commands.
Permissions as host policy.
Execution as terminal evidence.
Outputs as durable artifacts.
```

The model receives a small bridge such as `exec_command` and `write_stdin`. It does not receive PowerShell, `cmd.exe`, Bash, Termux, host filesystem paths, provider credentials, or unrestricted process access.

## Hybrid Runtime Direction

ReadOS uses a hybrid MSP architecture:

1. The product host owns policy, approval, sessions, transcripts, audit, artifacts, provider access, and domain services.
2. A portable Rust command runtime owns deterministic parsing, expansion, virtual command dispatch, binary-safe results, and resource bounds.
3. Platform backends own safe filesystem, process, PTY, and platform integration.
4. A small portable builtin command pack operates directly on the virtual workspace.
5. Complex mature tools such as Git, Python, Node, Toybox, BusyBox, or Termux packages run only through verified and policy-controlled external runtime providers.

The authoritative architecture and ownership rules are in [docs/MSP_HYBRID_ARCHITECTURE.md](docs/MSP_HYBRID_ARCHITECTURE.md).

## Initial Vertical Domain

The document workbench must support a complete supervised workflow:

```text
import document
→ inspect/select evidence
→ prepare an MSP command or workflow
→ approve external or mutating effects
→ execute with progress and cancellation
→ persist transcript, audit, artifact, and provenance
→ restart and recover
→ navigate lineage back to the source document and page
```

The flagship workflow must work through the same host/runtime boundary used by future vertical applications.

## Product Principles

1. Vertical depth before platform breadth.
2. Product authority stays above the native runtime.
3. The virtual workspace is the agent-facing source of truth.
4. Portable behavior is shared; platform capabilities are explicit.
5. Builtins implement only safe, useful MSP subsets.
6. Mature external tools are integrated, not rewritten.
7. No arbitrary shell string execution.
8. Capability claims require executable evidence on the target platform.
9. Security failures are fail-closed and do not disclose host paths or secrets.
10. A runtime feature is not complete until a product or SDK consumer can use it through the supported boundary.

## Product Boundary

ReadOS owns:

- the WinUI operator workbench;
- document/PDF services and workspace persistence;
- chat/provider integration and credentials;
- app command packs and named workflows;
- policy, approval, terminal audit, sessions, transcripts, artifacts, and lineage;
- packaging and product-level acceptance.

The portable MSP runtime owns:

- command parsing and expansion;
- virtual paths and backend contracts;
- deterministic command registration and dispatch;
- bounded binary stdin/stdout/stderr;
- stable execution results and diagnostics;
- FFI-safe runtime contracts.

Platform adapters own:

- safe host workspace binding;
- process and PTY lifecycle;
- platform storage APIs;
- runtime bundle discovery and verification;
- platform-specific sandbox and resource enforcement.

## Non-Goals

- Do not recreate a complete POSIX userland.
- Do not expose arbitrary system shell access to the model.
- Do not make Termux, PowerShell, Bash, or host `PATH` part of the portable core.
- Do not move PDF, chat, provider, credential, or UI logic into Rust.
- Do not claim Windows/Linux/Android parity from compilation alone.
- Do not keep two native runtime implementations indefinitely.

## Definition of Product Success

ReadOS succeeds when:

- the Windows flagship workflow passes through a stable supported runtime boundary;
- the portable command runtime is reused unchanged by Windows, Linux, and Android hosts;
- platform backends truthfully expose different capabilities without changing core semantics;
- verified external runtime providers can add mature tools without granting arbitrary shell access;
- the legacy native core is retired after compatibility and package gates pass; and
- an operator can understand what the agent attempted, what was authorized, what changed, and where every artifact came from.

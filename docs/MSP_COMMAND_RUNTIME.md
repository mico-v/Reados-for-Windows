# MSP command runtime boundary

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). This document describes the current portable runtime slice only.

`msp-command-runtime` is the native ReadOS-owned orchestration slice between
`msp-kernel::CommandPlanner` and `msp-command-pack::Registry`. It accepts only
caller-provided virtual inputs and executes deterministic virtual builtins over a
caller-provided `msp-backend::WorkspaceBackend`.

## Boundary and ownership

The managed host retains the raw request and owns policy, approval, transcript,
terminal routing, and exactly-once product audit. The runtime has no policy or
audit authority. A host calls `prepare`, inspects `ExecutionMetadata`, performs
its decision, and calls `execute` only after approval.

The runtime never accesses a host current directory, environment, filesystem,
`PATH`, process API, managed code, or product service. `VirtualPath` values are
virtual namespace values and are never converted to host paths. The registry is
used only to establish registration and effect; arbitrary command-pack
summaries are not retained or exposed.

`PreparedCommand` hides expanded arguments and source words. Its metadata and
execution summary contain only bounded counts, validated command/effect status,
virtual cwd access, byte counts, and status flags. Runtime diagnostics are
fixed codes and never echo command input, expansion values, parameters, paths,
or command-pack messages.

Cancellation is cooperative at planning, invocation, and dispatch boundaries.
Because the current command trait has no cancellation parameter and builtins may
run synchronously, cancellation observed after dispatch is reported as an
observation rather than an interruption claim.

# ReadOS virtual command runtime

`msp-command-runtime` is a stateless, native orchestration boundary over the
neutral `msp-kernel`, `msp-command-pack`, and `msp-backend` crates. It plans a
borrowed raw command using a caller-owned `ExpansionContext`, projects the plan
into opaque sanitized metadata, and dispatches only registered virtual builtins
through an explicitly supplied `WorkspaceBackend` and `Registry`.

The crate has no process execution, shell, host environment, host filesystem,
PATH lookup, managed-code dependency, policy authority, approval authority,
audit sink, product route, or global configuration. Every execution input is
supplied by the caller: expansion context, virtual cwd, backend, registry,
stdin, command limits, and optional cooperative cancellation state.

The intended hosting sequence is:

1. The host retains the authoritative raw command for policy, approval,
   transcript, and audit.
2. The host calls [`CommandRuntime::prepare`] and inspects the bounded,
   content-free [`ExecutionMetadata`].
3. The host performs policy and approval itself. If denied, it does not execute.
4. The host calls [`CommandRuntime::execute`].
5. The host owns terminal-result projection and exactly-once product audit.

`PreparedCommand` does not expose raw words, raw source, expanded arguments,
parameter names, or expansion values. Runtime diagnostics and summaries are
fixed-code and path-free. Command stdout/stderr remain binary-safe bounded
bytes. Cancellation is cooperative at the API boundary; if it is observed
after synchronous registry dispatch begins, the result records an observation
rather than claiming that the command was interrupted.

This crate is a ReadOS-owned execution slice for deterministic virtual
builtins, not a second managed runtime.

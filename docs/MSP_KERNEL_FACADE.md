# ReadOS kernel facade provenance

`native/msp-kernel` and `native/msp-host-adapter` are ReadOS-owned contracts introduced
for the Phase 0/1 kernel boundary. Their source is authored in this repository and is
not copied from the upstream MSP workspace or from `MSP/`.

The facade intentionally contains only owned Rust values and synchronous traits. It
has no dependency on `native/msp-core`, `native/msp-ffi`, `MSP/`, operating-system
APIs, managed runtime types, or C callback types. The host adapter currently performs
request and event projection only. `LegacyKernelAdapter` returns
`KernelError::ExecutionUnavailable` instead of claiming to delegate execution; the
existing managed control plane and Hosting adapter therefore remain authoritative.

The root workspace contains five new crates: `native/msp-kernel`,
`native/msp-host-adapter`, `native/msp-shell-language`,
`native/msp-shell-expansion`, and `native/msp-protocol-windows`. The first two
define ReadOS-owned kernel and host-adapter contracts. The shell-language crate is a standalone,
host-shell-free parser/AST boundary with its upstream provenance recorded in
`native/msp-shell-language/NOTICE`. The protocol crate is a ReadOS-owned,
pure byte-slice JSON framing and DTO boundary: it validates versioned requests,
projects kernel events into safe wire values, and translates canonical commands
and virtual roots one way into kernel requests. It has no process, filesystem,
Windows API, ABI, or managed-runtime dependency and never performs execution,
policy, audit, or cancellation-state lookup. Its package-local `NOTICE` records
that the code is authored here and contains no copied upstream implementation.

The standalone manifests and lockfiles for `native/msp-core` and `native/msp-ffi`
remain outside that workspace so existing build scripts and the public C ABI are
unchanged.

## Non-executing command planning

`msp-kernel::CommandPlanner` is the kernel's syntax-to-plan boundary. Its
`plan` method first enforces a 128 KiB UTF-8 input bound, then calls
`ShellParser::parse_supported_simple_command`, and expands the parsed command
and argument words with the caller-owned `msp_shell_expansion::ExpansionContext`.
It returns a `CommandPlan` containing expanded `program` and `args`, the bounded
original `raw_command`, plus `PlannedWord` metadata containing bounded raw-word
text, expanded text, segment lengths, quote flags, source kinds, and the explicit
empty-quote marker. Per-word, aggregate-byte, word-count, and segment-count
limits are enforced before a plan is returned.

Planning is intentionally separate from execution. It does not construct an
execution DTO, emit `KernelEvent`, invoke `EventSink` or `KernelExecutor`, read
process environment state, inspect the filesystem, resolve `PATH`, or consult a
current directory. Scalar values are not field-split, so every parsed word
produces exactly one planned value, including an empty quoted argument.

`CommandPlanError` exposes only stable parser, expansion, validation, and limit
categories with word positions or argument indexes. Error display/debug output
never includes command text, parser messages, expansion operands, parameter names,
parameter values, or invalid command/argument values. Unsupported
command/process substitution, arithmetic expansion, pathname expansion, unbound
parameters (when selected by the context), and invalid command names are
rejected before any execution-facing layer. Kernel command-name validation
accepts ASCII letters/digits plus `-`, `_`, and `.`; stricter lowercase/length
policy remains the responsibility of the wire protocol or managed policy layer.

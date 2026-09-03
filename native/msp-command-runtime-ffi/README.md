# ReadOS command runtime C ABI

`msp-command-runtime-ffi` is the ReadOS-owned, target-neutral C ABI for deterministic virtual command execution. It exposes opaque runtime, workspace, and result handles over the frozen `reados-portable-msp-v1` profile: `pwd`, `echo`, `printf`, `cat`, `ls`, `find`, `du`, `head`, `tail`, `wc`, `grep`, and the selected literal `sed` subset. This is a deliberately bounded local subset, not full upstream MSP command parity.

The versioned ABI source of truth is `abi/msp_command_runtime_ffi.v1.json`. Keep the C header, export definition, Rust surface, .NET adapter, and Android constants aligned with that manifest; `scripts/verify-msp-command-runtime-ffi-abi.ps1` is the repository gate for drift.

This ABI has no process execution, host filesystem, environment lookup, callbacks, audit/policy authority, host paths, or dependency on the legacy `msp-ffi` ABI. The authored header is `include/msp_command_runtime_ffi.h` and every symbol starts with `msp_command_runtime_ffi_`.

Requests are explicit-length UTF-8 JSON documents. Raw NUL bytes in the request are rejected, as are NULs decoded in command, cwd, and variable names or values. The `stdinBase64` field is different: its decoded value is arbitrary binary, so NUL and `0xFF` bytes are preserved. Workspace file data is also arbitrary binary. Outputs are length-delimited binary buffers and diagnostics are compact fixed-code JSON. Handles and output storage are owned by the caller according to the header contract.

The package is Apache-2.0 licensed and is separate from the MIT-licensed `native/msp-ffi` crate.

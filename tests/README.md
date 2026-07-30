# Tests

The active test projects are:

```text
tests/ReadOS.Msp.Tests
tests/ReadOS.Msp.Hosting.Tests
tests/ReadOS.App.Tests
```

Verified baseline on 2026-07-11:

- `ReadOS.Msp.Tests`: 69 tests;
- `ReadOS.Msp.Hosting.Tests`: 259 tests, including 3/3 real release-DLL operations;
- `ReadOS.App.Tests`: 300 tests;
- managed total: 628 tests;
- Rust `native/msp-core`: 52 tests, plus native binary and FFI smoke verification.

They cover MSP parser/runtime behavior, complete terminal audit records, structured diagnostics, streaming command events, namespace/path confinement, host-neutral request/approval/session/artifact services, transcript progress state, durable session records, workflow summaries, artifact provenance manifests, app virtual workspace projections, approval-gated host commands, DPAPI credential storage/migration/export exclusion, and chat command persistence. New MSP work should add tests close to the layer being changed:

- parser tests for quoting, argument shape, and invalid command text;
- runtime tests for success, failure, unknown commands, dry runs, and exit codes;
- diagnostic tests for stable codes, targets, and recovery hints on failure paths;
- event tests for started, policy, progress, completed, and canceled streaming states;
- workspace tests for virtual path normalization, listing, file reads, and ReadOS projections such as `/sessions` and `/transcripts`;
- session/transcript tests for grouping, workflow summaries, diagnostics, recovery hints, progress/cancel state, and stale record cleanup surfaced in the workbench;
- policy tests for mutating command allow/confirm/deny behavior and approval previews;
- artifact tests when commands create durable outputs, including returned artifact metadata, automatic source provenance from PDF/chat evidence, and `/artifacts/*.manifest.json` projections.
- namespace security tests for parent traversal, backslashes, absolute/drive paths, prefix siblings, record identifiers, and manifest aliases;
- credential tests for protected storage, provider scoping, legacy migration, missing credentials, export sanitization, and secret-free logs/output.

T93 service-level integration covers PDF import, outline selection, approval across restart, evidence extraction, synthesis, durable session/transcript/artifact/manifest recovery, denial without partial output, lineage back to a source page, cancellation, invalid pages, and secret-safe provider failure/restart/retry. Remaining coverage must exercise visible WinUI interaction, real provider/network behavior, and the flagship workflow across a packaged-process restart before T93 can close. T94 layout tests also protect compact/medium/wide width restoration; Runtime Drawer and asynchronous latest-request-wins coverage remain open.

T95 now verifies the handle-based read-only NTFS WorkspaceFS, path sanitization, stable Hosting adapter, static CRT, real DLL integration, `native_pwd_echo_adoption_v1`, and the completed `native_abi_v2_handshake_v1` slice. ABI coverage fixes the 32-byte `2.0` handshake (`0x324D534F44414552`, capabilities `0xF`), seven exports, length-delimited embedded-NUL-safe buffers, all-three-exports-missing-only v1 fallback, fail-closed partial/drift cases, separate allocators, free exactly once on every native-return path, serialized invoke/dispose, and runtime ABI evidence. Parse/Execute/Normalize request caps are 128 KiB/1 MiB/1 MiB and response caps are 16 MiB/64 MiB/1 MiB; bounded-writer tests cover the fix for the measured 54.5x Parse amplification path. The no-skip package gate adds staged v2/v1 FFI and real packaged-process ABI 2.0 evidence; the verified ZIP is `artifacts/releases/ReadOS-0.1.0-native-abi-v2-verified-win-x64.zip`. Next tests should preserve exact behavior while replacing hard-coded Rust dispatch in `native_command_core_registry_v1`, then cover the mixed-workspace read bridge before product `ls`/`cat`, and the bounded byte-stream core before mutable WorkspaceFS, pipelines/sessions/ConPTY, and broader upstream conformance. The nested `MSP/` checkout remains a local-only reference and must not enter ReadOS Git or package output.

Run all managed projects explicitly or use the full fail-fast verifier:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
.\scripts\verify-msp.ps1
```

# Tests

The active test projects are:

```text
tests/ReadOS.Msp.Tests
tests/ReadOS.Msp.Hosting.Tests
tests/ReadOS.App.Tests
```

Local diagnostic baseline observed on 2026-08-26:

- `ReadOS.Msp.Tests`: 69 tests;
- `ReadOS.Msp.Hosting.Tests`: 408 passed and six explicit real-DLL cases skipped because release DLL environment variables were not configured;
- `ReadOS.App.Tests`: 425 tests;
- managed total in this no-explicit-DLL run: 902 passed and six skipped;
- Rust `native/msp-core`: 308 tests (301 unit tests plus 7 PTY integration tests), plus native binary and FFI smoke verification.

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

H6 product-level integration covers PDF import, outline selection, approval across restart, evidence extraction, synthesis, durable session/transcript/artifact/manifest recovery, denial without partial output, lineage back to a source page, cancellation, invalid pages, and secret-safe provider failure/restart/retry. Remaining coverage must exercise visible WinUI interaction, a real provider/network boundary, and the flagship workflow across a packaged-process restart.

Legacy native tests remain compatibility and migration evidence for H1-H7. New portable behavior belongs in the modular Rust workspace, while Windows/Linux/Android backend claims require real target-platform integration evidence. The nested `MSP/` checkout remains a local-only reference and must not enter ReadOS Git or package output.

Run all managed projects explicitly or use the full fail-fast verifier:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
.\scripts\verify-msp.ps1
```

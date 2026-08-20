# Source Layout

This directory contains the current MSP vertical service implementation.

```text
src/
  ReadOS.App/
    WinUI operator workbench, document/PDF/chat adapters, secure provider
    credential storage, app command pack, and observable UI projection.
  ReadOS.Msp/
    Portable .NET MSP runtime, command contracts, workspace abstraction,
    policy/audit interfaces, parser, and core command pack.
  ReadOS.Msp.Hosting/
    Host-neutral command-host composition, request/approval orchestration,
    sessions/transcripts, cancellation, and artifact catalog/provenance services.
```

The app hosts MSP in process by combining the Hosting layer with its domain adapters:

```text
src/ReadOS.App/Services/Msp/
  ReadOsMspHost.cs
  ReadOsMspCommands.cs
  ReadOsVirtualWorkspace.cs
```

Keep app-specific document, PDF, chat, and UI logic in `ReadOS.App`. Keep runtime-neutral command, workspace, policy, audit, and result contracts in `ReadOS.Msp`.

The Hosting split is already active:

```text
src/ReadOS.Msp.Hosting/
```

Add code there only when it is host-neutral and can be tested without WinUI or ReadOS document models. Session projection, command-host composition, approvals, cancellation, and artifact catalog/provenance services meet that rule today. Workspace persistence, provider credentials, PDF/chat implementations, and UI state remain app-owned.

Provider API keys are deliberately outside the virtual workspace and workspace export boundary. `ReadOS.App` hydrates them through `IProviderCredentialStore`; the Windows implementation protects per-provider values with DPAPI `CurrentUser` and migrates legacy plaintext workspace settings.

T95's native direction is implemented outside `src/`, in `native/msp-core`, with a stable .NET adapter remaining on the managed side of this ownership boundary. Its behavior is derived from the local Apache-2.0 `MSP/Spec`, available `MSP/Conformance` fixtures/reference outputs, and committed ReadOS snapshots. The 2026-08-18 Windows source package is a read-only review input with 24 crates and two examples; its `SOURCE-PACKAGE-README.md` excludes Mac/Swift implementation source, and the documented parity inventory/release runners are incomplete. See `conformance/msp-upstream/windows-capability-manifest.json` for blocked evidence and drift records. Do not copy the raw reference repository into ReadOS output. Any copied or derived code must carry NOTICE and source-provenance records; `MSP/` itself is excluded from ReadOS packages.

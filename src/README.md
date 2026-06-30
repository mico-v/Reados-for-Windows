# Source Layout

This directory contains the current MSP vertical service implementation.

```text
src/
  ReadOS.App/
    WinUI operator workbench, document UI, service adapters, and host wiring.
  ReadOS.Msp/
    Portable .NET MSP runtime, command contracts, workspace abstraction,
    policy/audit interfaces, parser, and core command pack.
```

The app project currently hosts MSP in process through:

```text
src/ReadOS.App/Services/Msp/
  ReadOsMspHost.cs
  ReadOsMspCommands.cs
  ReadOsVirtualWorkspace.cs
```

Keep app-specific document, PDF, chat, and UI logic in `ReadOS.App`. Keep runtime-neutral command, workspace, policy, audit, and result contracts in `ReadOS.Msp`.

Future extraction target:

```text
src/ReadOS.Msp.Hosting/
```

Use that project only after session handling, policy, transcript, and artifact boundaries are stable enough to test outside the WinUI app.

# ReadOS

ReadOS is an MSP-first vertical application service and Windows workbench. Its purpose is to prove how an AI agent can operate inside real product software through an app-owned command runtime, virtual workspace, domain command packs, policy, audit, and durable artifacts.

The existing document/PDF reading experience is now the first vertical domain for MSP rather than the final product boundary. It gives the runtime rich materials, conversations, page evidence, search, attachments, and user-visible workflows to operate on.

The product direction is maintained in [PRODUCT_GOAL.md](PRODUCT_GOAL.md), and the implementation path is maintained in [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md).

## Current Status

- WinUI 3 Windows workbench under `src/ReadOS.App`.
- Portable .NET MSP runtime under `src/ReadOS.Msp`.
- MSP parser, command registry, runtime context, workspace abstraction, policy interface, audit sink, and basic commands: `help`, `pwd`, `echo`, `ls`, `cat`.
- ReadOS host adapter under `src/ReadOS.App/Services/Msp`.
- ReadOS virtual workspace exposing `/settings.json`, `/projects`, `/library`, and `/documents`.
- Domain commands for `workspace info`, `library list`, and `pdf inspect|text|search`.
- Document services for PDF rendering, text extraction, search, page labels, outlines, attachments, and per-document conversations.
- OpenAI-compatible chat service and offline fallback.
- Rust native MSP core prototype under `native/msp-core`.
- MSP xUnit tests under `tests/ReadOS.Msp.Tests`.

## Architecture Direction

ReadOS should evolve into a vertical MSP service host with these layers:

1. Operator workbench: WinUI surface for workspace, evidence, command transcript, and approval.
2. MSP service host: sessions, command execution, policy checks, audit records, and artifact lifecycle.
3. Command runtime: parser, command registry, exit codes, stdout/stderr, and future composition features.
4. Virtual workspace: app-owned file model projected as stable MSP paths.
5. Domain command packs: document, PDF, chat, artifact, workflow, and future app-specific commands.
6. Agent bridge: a small model-facing boundary such as `exec_command({ "cmd": "pdf search current \"policy\"" })`.
7. Native core and SDK: portable runtime pieces extracted behind stable JSON contracts.

## Build, Test, And Run

Build only:

```powershell
.\scripts\run.ps1 -BuildOnly
```

Build and run the workbench:

```powershell
.\scripts\run.ps1
```

Run managed MSP tests:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
```

Run the full MSP verification path:

```powershell
.\scripts\verify-msp.ps1
```

Create a Windows x64 release package:

```powershell
.\scripts\package-windows.ps1 -StopExisting
```

## Development Documents

- [PRODUCT_GOAL.md](PRODUCT_GOAL.md): MSP vertical service product direction.
- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md): practical implementation roadmap.
- [docs/MSP_PLAN.md](docs/MSP_PLAN.md): service architecture and runtime model.
- [docs/MSP_SDK_DEVELOPMENT_PLAN.md](docs/MSP_SDK_DEVELOPMENT_PLAN.md): SDK and native extraction plan.
- [docs/UI_UX_DESIGN.md](docs/UI_UX_DESIGN.md): desktop conversation workbench UI/UX design.
- [docs/MSP_AGENT_COMMAND_LOOP.md](docs/MSP_AGENT_COMMAND_LOOP.md): prompt-injected MSP command loop.
- [docs/APP_FRAME_DESIGN.md](docs/APP_FRAME_DESIGN.md): current WinUI frame notes.
- [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md): Windows, WinUI, and CLI setup.

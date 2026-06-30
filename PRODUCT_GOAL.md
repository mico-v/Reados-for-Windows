# ReadOS Product Goal

ReadOS is the vertical application service for Model Shell Proxy (MSP). It should be a real product-shaped host where an AI agent operates through commands, files, policies, audit records, and artifacts rather than through a loose set of one-off tools.

The original PDF reading product remains important, but its role changes: document work becomes the first vertical domain that proves MSP can serve deep application workflows.

## Mission

Build the reference vertical service for MSP:

- expose app state as a stable virtual workspace;
- expose app capabilities as composable commands;
- make every agent action inspectable, policy-controlled, replayable, and auditable;
- turn generated outputs into durable artifacts;
- provide a usable operator workbench for humans supervising the agent.

## Product Positioning

ReadOS is not a generic chat app, a generic PDF reader, or a raw SDK demo. It is a service workbench for agent-native software.

The target users are:

- builders who need to see how MSP fits into a real application;
- power users who want agents to work over documents, conversations, and artifacts;
- future vertical-app teams that need reusable MSP host patterns.

## Core Thesis

MSP should make applications feel like controlled operating environments for agents:

```text
Data as files.
Actions as commands.
Permissions as policy.
Execution as evidence.
Outputs as artifacts.
```

The app owns the runtime. The agent receives a small bridge such as `exec_command`, while the service host handles command parsing, dispatch, workspace projection, policy decisions, audit records, and artifact persistence.

## Design Principles

1. MSP first: every significant capability should have a command/runtime shape, not only a UI button.
2. Vertical depth before breadth: start with document workflows, then generalize proven patterns.
3. App-owned workspace: agents see virtual paths, never arbitrary host filesystem paths.
4. Commands over bespoke tools: app capabilities should compose through shell-like conventions.
5. Policy before mutation: write, export, delete, and external-call commands must pass explicit policy.
6. Evidence by default: command results, source paths, page references, and artifacts should be inspectable.
7. Human supervision: the workbench should expose transcripts, approvals, errors, and rollback points.
8. Portable core: parser, command protocol, policy/audit schemas, and conformance tests should move toward the native MSP core.

## Core Product Areas

### 1. MSP Service Host

The service host coordinates command execution sessions. It owns the command registry, workspace adapter, policy engine, audit sink, artifact store, and agent bridge.

Near-term shape:

- in-process host inside `ReadOS.App`;
- session-scoped command transcript;
- command metadata for read/write/external side effects;
- approval hooks for mutating commands.

Long-term shape:

- embeddable service library;
- optional local process boundary;
- stable JSON protocol for native and .NET callers.

### 2. Virtual Workspace

ReadOS should project domain state into MSP paths:

```text
/
/settings.json
/projects/{projectId}/info.json
/library/{documentId}.json
/documents/{documentId}/info.json
/documents/{documentId}/outline.json
/documents/{documentId}/pages/{page}.txt
/documents/{documentId}/conversations/{conversationId}.json
/artifacts/{artifactId}/...
```

The workspace is not just storage. It is the agent's read model, evidence surface, and artifact graph.

### 3. Command Packs

Command packs turn vertical capabilities into runtime vocabulary.

Initial packs:

- core: `help`, `pwd`, `ls`, `cat`, `echo`;
- workspace: `workspace info`;
- library: `library list`;
- PDF: `pdf inspect`, `pdf text`, `pdf search`;
- artifact: planned `artifact write`, `artifact list`, `artifact show`;
- chat/workflow: planned `attach`, `chat ask`, `workflow run`.

### 4. Policy, Audit, And Evidence

Every command should produce a structured result:

- exit code;
- stdout and stderr;
- artifacts;
- audit records;
- policy decision;
- source references when available.

Read-only commands can run freely. Mutating commands should support allow, confirm, and deny modes. External network/model calls should be visible and cancellable.

### 5. Operator Workbench

The WinUI app becomes the human control plane:

- workspace browser;
- document/evidence viewer;
- command transcript;
- approvals for write-capable commands;
- artifact preview;
- settings for providers, models, policy, and prompts.

The reader surface remains useful, but it is now one view inside a broader MSP workbench.

### 6. SDK And Native Runtime

`src/ReadOS.Msp` should mature as the .NET MSP SDK. Stable language-neutral pieces should gradually move into `native/msp-core`:

- command request/result JSON protocol;
- parser and quoting rules;
- policy and audit schema;
- conformance fixtures;
- FFI boundary for host languages.

Host adapters stay app-specific. The native core should not know about WinUI, PDF libraries, local UI state, or provider secrets.

## Initial Vertical Domain: Documents

Documents are the first domain because they provide real, inspectable work:

- read workspace and library state;
- inspect PDF metadata, labels, outlines, and current page;
- search and extract text from page ranges;
- attach pages or regions to conversations;
- generate summaries, study notes, outlines, and evidence-backed answers;
- persist outputs as artifacts.

Document reading remains valuable, but the deeper goal is to validate MSP as the runtime boundary for vertical applications.

## Development Priorities

### Foundation

- Align documentation and naming around MSP service direction.
- Harden MSP command contracts and tests.
- Add command metadata for mutability, required capabilities, and output shape.
- Persist command transcripts and artifacts.

### Service Host

- Introduce a service-facing abstraction around `ReadOsMspHost`.
- Add session IDs, cancellation, progress events, and approval requests.
- Replace allow-all policy in app flows with configurable policy.

### Vertical Commands

- Expand document commands beyond inspection.
- Add write commands for page labels, outline edits, attachments, and artifacts.
- Add workflow commands that compose document, chat, and artifact operations.

### SDK Extraction

- Stabilize JSON contracts.
- Add conformance fixtures shared by .NET and Rust.
- Move runtime-neutral logic into `native/msp-core` only after behavior is proven in the vertical app.

## Non-Goals

- Do not expose arbitrary system shell access as MSP.
- Do not build a generic tool-calling catalog.
- Do not make PDF reading the final boundary of the project.
- Do not let agents mutate user data without policy and audit.
- Do not move app-specific document logic into the native core.

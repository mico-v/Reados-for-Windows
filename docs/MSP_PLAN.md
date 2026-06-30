# MSP Vertical Service Architecture

This document defines the target architecture for ReadOS as a vertical application service for Model Shell Proxy (MSP).

## Problem

Most agent integrations expose scattered tools. That is enough for demos, but weak for real application work: tools do not share a filesystem model, command grammar, policy layer, artifact store, or replayable evidence trail.

MSP solves this by giving the agent a shell-like command environment owned by the application. ReadOS proves that idea in a vertical domain before the SDK is generalized.

## Architectural Boundary

MSP is not a wrapper around PowerShell, `cmd.exe`, Bash, or arbitrary host binaries. The application owns:

- command parsing and dispatch;
- virtual paths;
- command registry;
- policy checks;
- audit records;
- artifact persistence;
- domain service access.

The model-facing surface should remain small:

```text
exec_command({ "cmd": "pdf search current \"attention mechanism\"" })
```

Internally, the result can remain structured:

```json
{
  "exitCode": 0,
  "stdout": "12\t...preview...\n",
  "stderr": "",
  "artifacts": [],
  "auditRecords": []
}
```

## Logical Layers

```text
Agent / automation client
  -> Agent bridge
  -> MSP service host
  -> Policy gate
  -> Command runtime
  -> Command packs
  -> Virtual workspace
  -> Domain services
  -> Artifact and audit stores
```

### Agent Bridge

The bridge translates model requests into service-host calls. It should support:

- command text execution;
- session identity;
- cancellation;
- optional streaming events;
- structured result return.

### MSP Service Host

The host is the orchestration layer above the raw runtime. It owns:

- sessions;
- transcript records;
- command metadata lookup;
- policy authorization;
- audit sink;
- artifact store;
- approval requests for mutating commands.

### Command Runtime

The runtime owns:

- parsing;
- command registry lookup;
- working directory resolution;
- stdout/stderr conventions;
- exit codes;
- exception-to-result conversion;
- future composition features such as pipes and redirection.

### Command Packs

Command packs expose app capabilities as shell primitives.

Core pack:

- `help`
- `pwd`
- `ls`
- `cat`
- `echo`

ReadOS document pack:

- `workspace info`
- `library list`
- `pdf inspect`
- `pdf text`
- `pdf search`

Planned packs:

- attachment commands;
- chat commands;
- workflow commands.

Started packs:

- artifact commands;
- page label commands;
- outline commands;

### Virtual Workspace

The virtual workspace is the agent's file model. It must be stable, readable, and independent of host paths.

Current and target path shape:

```text
/
/settings.json
/projects
/projects/{projectId}/info.json
/library
/library/{documentId}.json
/documents
/documents/{documentId}/info.json
/documents/{documentId}/outline.json
/documents/{documentId}/pages/{page}.txt
/documents/{documentId}/conversations/{conversationId}.json
/artifacts
/artifacts/{artifactId}/manifest.json
/artifacts/{artifactId}/content.md
```

Domain services can be more complex internally, but the agent should see durable, inspectable files.

## Runtime Contracts

### Command Request

Minimum fields:

- `commandText`
- `workingDirectory`
- `actor`
- `dryRun`
- `sessionId`

### Command Result

Minimum fields:

- `exitCode`
- `stdout`
- `stderr`
- `artifacts`
- `auditRecords`
- `diagnostics`

### Command Metadata

Every command should declare:

- command name and summary;
- read-only or mutating behavior;
- external network/model behavior;
- expected artifact outputs;
- required capabilities;
- argument usage string.

Metadata lets the workbench explain commands before execution and lets policy decide without parsing command-specific internals each time.

## Policy Model

Initial policy modes:

- allow: safe read-only commands;
- confirm: writes, exports, deletes, model calls, or external effects;
- deny: unsupported, dangerous, or disabled capabilities.

Policy requests should include:

- actor;
- session;
- command name;
- full command text;
- working directory;
- declared side effects;
- target paths;
- dry-run flag.

The current `AllowAllMspPolicy` is acceptable for tests and early development only.

## Audit And Evidence

Audit records should answer:

- who ran the command;
- what command ran;
- when it ran;
- what policy decided;
- what workspace path or document it touched;
- whether it succeeded;
- what artifacts were created.

Evidence should be visible both as UI transcript and as workspace data that future commands can read.

## Artifact Model

Artifacts are durable command outputs. Examples:

- summaries;
- extracted evidence tables;
- generated Markdown notes;
- cropped page images;
- exported page ranges;
- structured JSON reports.

Each artifact should have:

- path;
- media type;
- description;
- source command;
- source document/page references;
- creation time;
- actor/session.

## Workflow Model

Workflows should be added after single-command semantics are reliable. A workflow is a named, inspectable sequence of MSP commands, not a hidden block of app logic.

Example future workflow:

```text
workflow run explain-section --document current --outline "3.2"
```

Possible internal steps:

1. inspect current PDF;
2. resolve outline section pages;
3. extract text and page evidence;
4. attach evidence;
5. call chat model;
6. write `/artifacts/.../explanation.md`;
7. record audit and provenance.

## Implementation Path

1. Harden .NET contracts in `src/ReadOS.Msp`.
2. Add service-host concepts around `ReadOsMspHost`.
3. Persist transcripts and artifacts.
4. Add policy metadata and approval UI.
5. Expand document command packs.
6. Add conformance fixtures for request/result behavior.
7. Extract runtime-neutral pieces into `native/msp-core`.

## Design Rule

If a capability matters to the agent, it should eventually be visible in one of three places:

- a virtual workspace path;
- a command;
- an artifact with provenance.

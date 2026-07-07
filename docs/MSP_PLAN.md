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
  "diagnostics": [],
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

Current status: the .NET runtime and ReadOS host expose `ExecuteStreamingAsync`, which emits started, policy decision, progress, completed, and canceled events for agent bridge consumers. The workbench consumes those events to update transcript progress, record the final result, and cancel the active MSP command.

### MSP Service Host

The host is the orchestration layer above the raw runtime. It owns:

- sessions;
- transcript records;
- command metadata lookup;
- policy authorization;
- audit sink;
- artifact store;
- approval requests for mutating commands;
- command lifecycle and progress event publication.

### Command Runtime

The runtime owns:

- parsing;
- command registry lookup;
- argument-specific command metadata lookup;
- working directory resolution;
- stdout/stderr conventions;
- exit codes;
- exception-to-result conversion;
- command progress reports through the execution context;
- future composition features such as pipes and redirection.

### Command Packs

Command packs expose app capabilities as shell primitives.

Core pack:

- `help`
- `pwd`
- `ls`
- `cat`
- `echo`
- `workflow summary`
- `workflow run summarize-current`
- `workflow run review-failures`

ReadOS document pack:

- `workspace info`
- `library list`
- `pdf inspect`
- `pdf text`
- `pdf text ... --artifact /artifacts/excerpt.txt`
- `pdf search`
- `pdf search ... --artifact /artifacts/search.tsv`
- `workflow run explain-section --document current --outline "3.2" --artifact /artifacts/workflows/explanation.md`
- `workflow run extract-evidence --document current --outline "3.2" --artifact /artifacts/workflows/evidence.json`
- `workflow run review-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-review.md`
- `workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md`
- `workflow run refine-artifact --source /artifacts/workflows/evidence-synthesis.md --instruction "tighten caveats" --artifact /artifacts/workflows/evidence-synthesis-refined.md`

Planned packs:

- additional document workflow run commands.

Started packs:

- artifact commands;
- workflow summary commands;
- document workflow commands;
- page label commands;
- outline commands;
- attachment commands;
- chat commands;
- chat artifact output with queued evidence provenance;

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
/artifacts/{path}
/artifacts/{path}.manifest.json
/sessions
/sessions/{sessionId}.json
/transcripts
/transcripts/{transcriptId}.json
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
- `diagnostics`
- `artifacts`
- `auditRecords`

### Command Diagnostics

Diagnostics make failures machine-readable for the agent and actionable for the operator. Each diagnostic includes:

- severity;
- stable code such as `msp.command_not_found` or `msp.policy.require_confirmation`;
- message;
- optional target;
- recovery hint.

Current status: runtime failures include stable MSP codes for parse, unknown command, policy confirmation, cancellation, workflow lookup, and runtime exceptions. ReadOS command packs add domain-specific codes such as `reados.pdf.document_not_found` for unresolved PDF targets, `reados.pdf.invalid_page` and `reados.pdf.outline_item_not_found` for document metadata failures, and `reados.chat.model_provider_failed` for model-provider failures before any partial conversation or artifact write.

### Session Record

Session records group command transcripts and artifacts for a single agent/workbench execution context. Each record includes:

- session ID, title, actor, start and update times;
- last command text, decision, exit code, and progress message;
- last diagnostics summary and recovery hint;
- command and approval counts;
- failure count;
- transcript IDs;
- artifact paths.

### Command Events

Streaming execution emits lifecycle events:

- `Started`
- `PolicyDecision`
- `Progress`
- `Completed`
- `Canceled`

Events carry actor, session ID, command text/name, message, optional percent, exit code, final result, policy decision, effects, and preview. Commands report progress through `MspCommandContext.ReportProgressAsync`.

### Command Metadata

Every command should declare:

- command name and summary;
- read-only or mutating behavior;
- external network/model behavior;
- expected artifact outputs;
- required capabilities;
- argument usage string.

Metadata lets the workbench explain commands before execution and lets policy decide without parsing command-specific internals each time. Composite commands can override metadata per argument set; for example, `artifact list` is read-only while `artifact write` creates a durable artifact.

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
- host-controlled environment values such as approval tokens.
- command previews with summary, targets, and details.

The current app host uses effect-based confirmation plus one-shot approval tokens. `AllowAllMspPolicy` should remain limited to tests and explicitly trusted local experiments.

## Audit And Evidence

Audit records should answer:

- who ran the command;
- what command ran;
- when it ran;
- what policy decided;
- what workspace path or document it touched;
- whether it succeeded;
- what artifacts were created.
- what pre-execution preview the operator saw.

Evidence should be visible both as UI transcript and as workspace data that future commands can read.

Current workbench transcript records are persisted in workspace state and projected as read-only JSON files under `/transcripts`. Failed transcript records include diagnostic summaries and recovery hints. Session records are rebuilt from transcripts and artifacts and projected under `/sessions`, giving agents a compact history index with pending approval counts, failure counts, artifact references, and the latest recovery hint before reading individual transcript files. The workbench uses the same session projection to surface running, approval, failure, and completed state in the left context list. `workflow summary current|<session-id>` turns those projections into a workflow-level Markdown report, and `--artifact` persists that report with session/transcript source paths. Approval-gated commands also carry preview text with affected documents, pages, artifact paths, queued attachments, and model/provider details where available. Artifact manifests expose the command provenance that created durable outputs.

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

Current status: `artifact write` returns an `MspArtifact` with path, media type, size, description, source command, actor, session ID, timestamps, and preview. `pdf text ... --artifact <path>` creates extraction artifacts and automatically populates source document IDs, virtual page paths such as `/documents/{id}/pages/12.txt`, and page ranges such as `{id}:12-14`. `pdf search ... --artifact <path>` creates tab-separated hit artifacts and records the matched source pages. `chat ask ... --artifact <path>` creates Markdown answer artifacts and records queued evidence attachments as source document/page references. `workflow summary ... --artifact <path>`, `workflow run summarize-current --artifact <path>`, and `workflow run review-failures --artifact <path>` create workflow report artifacts and record `/sessions/{id}.json` plus `/transcripts/{id}.json` source paths. Named session workflow reports additionally read upstream artifact manifests referenced by the current session and inherit their source document/page provenance. `workflow run explain-section --artifact <path>` creates a document-section explanation artifact and records the resolved source document, page range, and virtual page text paths. `workflow run extract-evidence --artifact <path>` creates an `application/json` artifact with one evidence record per resolved source page and records the same source document/page provenance. `workflow run review-evidence --artifact <path>` reads structured evidence artifacts and writes Markdown review notes with evidence artifact, evidence manifest, and inherited source page provenance. `workflow run synthesize-evidence --artifact <path>` reads structured evidence artifacts, calls the configured chat model, and writes source-grounded Markdown synthesis with the same inherited provenance. `workflow run refine-artifact --artifact <path>` reads an existing artifact plus manifest, applies an operator instruction through the configured model, and writes a derived Markdown artifact with inherited source provenance. ReadOS persists those fields with the workspace artifact, exposes them through sidecar manifests such as `/artifacts/notes.md.manifest.json`, renders selected-artifact lineage in the Artifacts inspector for source artifacts, manifests, virtual pages, source documents, and page ranges, and lets the workbench preview, copy, export, or attach selected artifacts without changing the virtual artifact record.

## Workflow Model

Workflows should be added after single-command semantics are reliable. A workflow is a named, inspectable sequence of MSP commands, not a hidden block of app logic.

Current first steps:

```text
workflow summary current --artifact /artifacts/workflows/current.md
workflow run summarize-current --artifact /artifacts/workflows/current.md
workflow run review-failures --artifact /artifacts/workflows/failures.md
workflow run explain-section --document current --outline "3.2" --artifact /artifacts/workflows/explanation.md
workflow run extract-evidence --document current --outline "3.2" --artifact /artifacts/workflows/evidence.json
workflow run review-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-review.md
workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md
workflow run refine-artifact --source /artifacts/workflows/evidence-synthesis.md --instruction "tighten caveats" --artifact /artifacts/workflows/evidence-synthesis-refined.md
```

The `summary` form summarizes an existing MSP session directly. The `run summarize-current` form keeps the session summary inspectable as a named workflow command. The `run review-failures` form creates a focused recovery report from failed transcript entries. Both session workflows read session/transcript projections from the virtual workspace, write provenance-backed Markdown artifacts when `--artifact` is supplied, and inherit source document/page provenance from upstream artifacts referenced by the session. The ReadOS app command pack overrides `workflow` for document and artifact-composition workflows such as `run explain-section`, `run extract-evidence`, `run review-evidence`, `run synthesize-evidence`, and `run refine-artifact`; all other workflow forms delegate back to the core runtime command. The workbench Inspector can prepare these workflow command strings from selected outline sections, artifacts, or failed transcript diagnostics, but execution remains explicit through the Run inspector and normal MSP policy path.

Implemented section workflow:

```text
workflow run explain-section --document current --outline "3.2" --artifact /artifacts/workflows/explanation.md
```

Internal steps:

1. inspect current PDF;
2. resolve outline section pages;
3. extract text and page evidence;
4. pass section evidence as a temporary model attachment;
5. call the configured chat model;
6. write `/artifacts/.../explanation.md`;
7. record audit and source document/page provenance.

Implemented evidence workflow:

```text
workflow run extract-evidence --document current --outline "3.2" --artifact /artifacts/workflows/evidence.json
```

Internal steps:

1. resolve the document and outline selector;
2. derive the section page range;
3. extract each page as a separate evidence record;
4. write an `application/json` artifact;
5. record audit and source document/page provenance.

Implemented evidence review workflow:

```text
workflow run review-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-review.md
```

Internal steps:

1. read a structured evidence artifact;
2. parse evidence document, section, and page records;
3. generate Markdown review notes and a citation table;
4. write a `text/markdown` artifact;
5. inherit source document/page provenance from the evidence artifact manifest.

Implemented evidence synthesis workflow:

```text
workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md
```

Internal steps:

1. read a structured evidence artifact;
2. parse evidence document, section, and page records;
3. pass the evidence artifact as model context;
4. call the configured chat model;
5. write a `text/markdown` synthesis artifact;
6. inherit source document/page provenance from the evidence artifact manifest.

Implemented artifact refinement workflow:

```text
workflow run refine-artifact --source /artifacts/workflows/evidence-synthesis.md --instruction "tighten caveats" --artifact /artifacts/workflows/evidence-synthesis-refined.md
```

Internal steps:

1. read the source artifact and manifest;
2. pass the source artifact content plus operator instruction as model context;
3. call the configured chat model;
4. write a `text/markdown` derived artifact;
5. inherit source document/page provenance from the source artifact manifest.

## Implementation Path

1. Harden .NET contracts in `src/ReadOS.Msp`.
2. Add service-host concepts around `ReadOsMspHost`.
3. Persist transcripts and artifacts.
4. Add policy metadata, approval UI, and streaming command events.
5. Expand document command packs.
6. Add conformance fixtures for request/result/event behavior.
7. Extract runtime-neutral pieces into `native/msp-core`.

## Design Rule

If a capability matters to the agent, it should eventually be visible in one of three places:

- a virtual workspace path;
- a command;
- an artifact with provenance.

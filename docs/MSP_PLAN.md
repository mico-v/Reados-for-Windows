# Model Shell Proxy

The command runtime for agent-native software.

Model Shell Proxy, or MSP, is an attempt to define a common runtime standard for
how AI agents operate inside applications.

Today, most agent software exposes scattered tools: one function for reading a
file, another for searching, another for writing, another for app-specific work.
That does not scale into a real software environment.

MSP takes a different position:

Agents should operate through a shell-like command layer. Applications should own
the runtime. Every action should be inspectable, replayable, policy-controlled,
and portable.

The goal is not to wrap `/bin/sh`.

The goal is to define the command interface for the next generation of software.

## The Bet

The next software platform will not be a chat box with plugins.

It will be an app-owned operating environment where agents can inspect state, run
commands, create artifacts, compose workflows, and leave evidence behind.

Shells already gave software engineering a universal interaction model:
commands, files, streams, exit codes, scripts, and logs. MSP brings that model
into agent-native apps with modern boundaries:

- virtual workspaces instead of raw host filesystem access
- app-defined command packs instead of arbitrary machine control
- policy and audit as first-class runtime concepts
- platform adapters for iOS, iPadOS, macOS, visionOS, Android, Windows, and beyond
- conformance fixtures instead of one-off integration behavior

MSP is not a library feature. It is a proposal for a new software boundary: the
command runtime between AI agents and applications.

## Core Principles

```text
Data as files.
Actions as commands.
Permissions as policy.
Execution as evidence.
```

These are not slogans for the UI. They are the shape of the runtime.

The model should be able to inspect real workspace artifacts. App capabilities
should be exposed as commands. Permission decisions should live in explicit
policy layers. Execution should leave records that humans, tests, and future
agents can inspect.

## Not A Wrapper Around The System Shell

MSP does not hand `/bin/sh`, `bash`, or `zsh` to the model.

The shell parser, command dispatch, expansion behavior, redirection handling,
WorkspaceFS path resolution, command registry, policy checks, audit records, and
app capability boundaries are implemented inside MSP.

That distinction is the point.

A real system shell is powerful, but it is also host-specific, process-oriented,
unsafe by default, difficult to sandbox consistently across platforms, and tied
to machine paths and binaries that an app may not control.

MSP keeps the shell interaction model while replacing the execution substrate.

The model sees shell-like command text. The app owns the runtime.

## Why Commands Beat Tool Schemas

MSP is built on a simple bet:

AI agents are better suited to a command environment than to an endless catalog
of bespoke tool schemas.

Shell commands are already one of the most heavily represented software
interfaces in the training distribution of modern coding models. Models have
seen shell scripts, terminal sessions, man pages, CI logs, README examples,
build scripts, deployment scripts, and debugging transcripts.

That matters.

A command like this is not just a function call:

```text
find /notes -name "*.md" | xargs grep "invoice" > /reports/matches.txt
```

It carries decades of software convention:

- paths
- streams
- pipes
- redirection
- exit codes
- globbing
- quoting
- stdin and stdout
- composable text transformation
- inspectable intermediate files

Traditional tool calling gives the model a list of isolated API endpoints. MSP
gives the model an operating grammar.

Tool calling gives agents hands. MSP gives agents a software environment.

## Beyond Tool Protocols

MCP and function calling make tools reachable.

MSP makes capabilities composable.

Those are different layers. A tool protocol can tell a model that a function
exists. It can describe input JSON and return output JSON. But the composition
layer is still weak: every new combination tends to require another tool,
another schema, another adapter, or more reasoning burden inside the model.

MSP moves composition into the runtime.

Once a capability is registered as a command, it can participate in the same
environment as every other command: pipes, redirection, files, scripts, command
substitution, exit codes, and generic Linux-style utilities.

This is the difference between giving the model buttons and giving it a command
line.

## App Commands Become Shell Primitives

MSP does not limit applications to a fixed generic command set.

An app can register its own domain-specific commands into the same command
runtime as the Linux-like core command pack. Once registered, those commands are
not isolated tool calls. They become shell primitives.

That means app-specific capabilities can be combined with:

- pipes
- redirection
- glob patterns
- workspace files
- stdin and stdout
- exit codes
- command substitution
- scripts and multi-step workflows
- generic commands such as `cat`, `grep`, `find`, `sort`, `xargs`, and `tee`

This is where the leverage compounds.

A traditional tool has a fixed schema. It usually accepts a narrow input shape
and returns a narrow output shape. The model can call it, but composition mostly
happens outside the runtime.

A command is different. If it reads files, writes files, accepts stdin, emits
stdout, and follows exit-code conventions, it can participate in larger
workflows without the app author designing a new API for every combination.

Tools are endpoints. Commands are building blocks.

MSP turns app capabilities into composable runtime vocabulary.

## Production Validation: ReadOS and Readex

MSP is not only a theoretical interface or SDK exercise.

Its strongest validation comes from ReadOS, a production learning and work
environment where Readex is the AI agent operating inside the user's workspace.
ReadOS itself is not part of this open-source repository, but it is the product
experience that shaped MSP.

Readex proves the central MSP idea in a real application: an agent should not be
limited to a chat box or a scattered set of isolated tools. It should work inside
an app-owned operating environment where user materials, app capabilities,
generated artifacts, long-running tasks, and prior work all become part of one
coherent command runtime.

In ReadOS, Readex can work with many kinds of user resources:

- documents and PDFs
- webpages and saved web references
- videos, subtitles, frames, and downloaded media
- text notes and structured files
- images and generated artifacts
- prior conversations
- knowledge structures created around learning materials
- workspace files produced by previous agent actions

The important point is not that each capability exists as a separate button or
function call. The important point is that these capabilities can be expressed
through a command layer owned by the application.

That lets Readex do work such as:

- inspect a workspace before answering
- read source materials instead of guessing
- extract focused evidence from PDFs or videos
- turn generated outputs into persistent workspace files
- refer back to prior conversations as reusable artifacts
- organize knowledge around documents rather than only chat history
- combine app-specific actions with familiar command-line composition
- leave behind results that humans and future agents can inspect

This is the difference MSP is trying to standardize.

A traditional agent integration exposes tools as endpoints. ReadOS exposes an
environment. Readex does not merely call one-off APIs; it operates through a
command runtime where files, streams, commands, artifacts, policies, and evidence
all share the same interaction model.

ReadOS is the production product. Readex is the agent experience inside it. MSP
is the open standard and SDK work shaped by that product experience.

## Current Status

This repository is the open MSP standard and SDK work, shaped by the production
experience of ReadOS and Readex. It is still under active construction.

The current Swift implementation includes:

- a `ModelShellProxy` facade
- a WorkspaceFS boundary backed by an app-provided workspace directory
- a hand-written MSP shell parser and runtime layer
- a POSIX-like core command pack
- policy and audit extension points
- an agent bridge for command execution
- Swift unit and integration tests
- conformance fixtures and reference outputs
- iOS example applications

This repository does not contain the full ReadOS product source code. ReadOS and
Readex are referenced here as production validation for the MSP model, not as an
open-source reference implementation.

This is not yet a final MSP v1 compatibility claim. Shell/runtime parity,
cross-platform implementations, oracle refresh workflows, and polished example
apps are still ongoing work.

## Repository Layout

```text
ModelShellProxy/
|-- Package.swift
|-- Spec/
|   |-- AgentBridge/
|   |-- WorkspaceFS/
|   |-- Profiles/
|   |-- Commands/
|   |-- ExternalRunners/
|   |-- Extensions/
|   |-- Security/
|   `-- Audit/
|-- Conformance/
|   |-- Fixtures/
|   |-- Inventory/
|   |-- ReferenceOutputs/
|   |-- Golden/
|   `-- Scripts/
|-- Implementations/
|   |-- Swift/
|   |   `-- Sources/
|   |       |-- ModelShellProxy/
|   |       |-- MSPCore/
|   |       |-- MSPShell/
|   |       |-- MSPPOSIXCore/
|   |       |-- MSPPythonRuntime/
|   |       |-- MSPPythonEmbeddedRuntime/
|   |       |-- MSPApple/
|   |       |-- MSPAgentBridge/
|   |       |-- MSPCommandKit/
|   |       `-- MSPExternalRunner/
|   |-- AndroidKotlin/
|   `-- Windows/
|-- Examples/
|   |-- iOS/
|   |   |-- MSPPlaygroundApp/
|   |   `-- PhotoSorter/
|   |-- Apple/
|   |   |-- CustomCommandDemo/
|   |   |-- ExternalRunnerDemo/
|   |   |-- iOSMinimalApp/
|   |   `-- macOSCommandLineDemo/
|   |-- Android/
|   `-- Windows/
|-- Docs/
|   |-- SDK/
|   `-- DemoCandidates/
|-- Tests/
|   |-- Swift/
|   |   |-- Fixtures/
|   |   |-- Golden/
|   |   |-- Unit/
|   |   `-- Integration/
|   `-- SpecConformance/
`-- References/
    |-- ReadexShellSnapshot/
    `-- ReadexReadingAgentSnapshot/
```

Generated build directories, local scratch directories, and temporary artifacts
are intentionally omitted from this tree.

## What Each Directory Means

`Spec/` defines the public MSP contract. It describes the agent bridge,
WorkspaceFS, profiles, security model, external runner boundary, and
command-layer expectations.

`Conformance/` contains the evidence layer for the spec. It includes required
command fixtures, inventories, oracle outputs, parity cases, and scripts used to
check behavior against Linux-like references.

`Implementations/` contains runtime implementations. Swift is the active
implementation today. Android and Windows are reserved for future platform work.

`Implementations/Swift/Sources/ModelShellProxy/` is the public Swift facade. It
wires shell parsing, command dispatch, workspace state, policy, audit, and
profile registration into one SDK entry point.

`Implementations/Swift/Sources/MSPCore/` contains shared runtime primitives:
commands, command results, policy requests, audit records, workspace protocols,
path resolution, and filesystem types.

`Implementations/Swift/Sources/MSPShell/` contains shell language support:
parsing, AST structures, redirection syntax, expansion, compound forms, heredocs,
and shell grammar support.

`Implementations/Swift/Sources/MSPPOSIXCore/` contains the POSIX-like command
pack, including filesystem, text, search, comparison, metadata, data, numeric,
process, and utility commands.

`Implementations/Swift/Sources/MSPPythonRuntime/` defines the optional Python
command profile. It owns Python invocation planning, command registration, and
the shared `MSPPythonRuntime` backend protocol.

`Implementations/Swift/Sources/MSPPythonEmbeddedRuntime/` contains the
in-process Python backend boundary for iOS, iPadOS, macOS, and visionOS. The
current CPython engine dynamically binds to an app-supplied CPython library; it
does not make Python part of the default POSIX core profile. Python subprocess
entry points such as `subprocess.run(...)` are routed back through MSP command
execution so child commands stay inside the same workspace, policy, and audit
boundary.

`Implementations/Swift/Sources/MSPApple/` adapts Apple platform storage into an
MSP workspace. The agent sees virtual paths rooted at `/`; the app owns the real
directory and policy.

`Implementations/Swift/Sources/MSPAgentBridge/` connects model requests to the
MSP command surface. It keeps the model-facing interface small while preserving
structured command results internally.

`Examples/` contains app-shaped demonstrations. These are not just snippets;
they show how MSP fits into real product loops such as chat, transcript
timelines, workspace browsing, and app-specific commands.

`Docs/` contains SDK notes and future demo concepts. The demo candidates explain
how MSP can support vertical apps beyond document workflows.

`Tests/` contains Swift unit tests, Swift integration tests, and spec conformance
tests. The test layout is intentionally separate from implementation folders so
the standard and runtimes can evolve independently.

`References/` contains read-only source material and historical snapshots. These
snapshots are preserved for comparison, migration, and conformance work. They
are not the active MSP source layout and they are not an open-source release of
ReadOS or Readex.

## Swift Package Modules

The Swift package currently exposes these libraries:

- `ModelShellProxy`
- `MSPCore`
- `MSPShell`
- `MSPCommandKit`
- `MSPExternalRunner`
- `MSPAgentBridge`
- `MSPPOSIXCore`
- `MSPPythonRuntime`
- `MSPPythonEmbeddedRuntime`
- `MSPApple`

The common Apple-platform entry point is:

```swift
let shell = try ModelShellProxy
    .iOS(workspaceURL: workspaceURL)
    .enable(.posixCore)
```

Python is optional and must be enabled explicitly:

```swift
let pythonEngine = try MSPCPythonEngine(
    library: .path(cpythonLibraryURL),
    workspaceRootURL: workspaceURL,
    pythonHomeURL: cpythonHomeURL
)

let shell = try ModelShellProxy
    .iOS(workspaceURL: workspaceURL)
    .enable(.posixCore)
    .enable(.python(runtime: MSPPythonEmbeddedRuntime(engine: pythonEngine)))
```

An agent runtime can connect to the shell through:

```swift
let bridge = shell.execCommandBridge()
```

The agent-facing tool remains intentionally small:

```text
exec_command({ "cmd": "ls -la /" })
```

Internally, MSP can still preserve structured command results, audit records,
policy decisions, workspace state, and test fixtures.

## Quick Start

From the repository root, run the active Swift package tests:

```sh
swift test
```

That is the fastest way to exercise the current public MSP runtime surface:
parser, command dispatch, WorkspaceFS behavior, policy hooks, audit records,
Python profile boundaries, and integration tests.

To inspect a product-shaped integration, start with
`Examples/iOS/MSPPlaygroundApp`. It shows MSP in an app loop with chat,
transcript state, workspace browsing, command execution, and app-owned runtime
boundaries.

## Where To Start

If you want to understand the standard, start with `Spec/`.

If you want to understand what is currently implemented, start with
`Implementations/Swift/Sources/ModelShellProxy/`, then read `MSPCore`,
`MSPShell`, and `MSPPOSIXCore`.

If you want to understand behavior coverage, start with `Conformance/Inventory`
and `Conformance/Fixtures`.

If you want to see MSP in an app shape, start with `Examples/iOS/MSPPlaygroundApp`.

If you want to understand the architectural origin, read `References/`, but
treat it as reference material rather than the active source layout.

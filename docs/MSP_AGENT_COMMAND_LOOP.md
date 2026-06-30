# MSP Agent Command Loop

ReadOS now supports a minimal agent-facing MSP command loop.

## Prompt Injection

When a chat request is sent, `ShellViewModel` injects MSP instructions through `AiChatService`. The model is told that MSP commands are for the agent, not the user, and that ReadOS will execute them through the app-owned runtime.

Supported request format:

````text
```msp
workspace info
library list
pdf inspect current
pdf search current "keyword"
pdf text current 1 3
artifact write /artifacts/summary.md "summary"
artifact list /artifacts
artifact show /artifacts/summary.md
page-label set current 12 "iii"
outline add current 42 "Chapter 3" --level 1
windows info
windows path current
```
````

Each non-empty line is parsed as one MSP command. Comment lines beginning with `#` or `//` are ignored. ReadOS also accepts `<msp>...</msp>` and `<reados-msp>...</reados-msp>` blocks.

## Execution Flow

1. User sends a normal chat prompt.
2. `AiChatService` injects the MSP command instructions.
3. If the model returns an `msp` block, `ShellViewModel` parses it.
4. Each command is executed through `ReadOsMspHost`.
5. Results are written to the MSP transcript panel.
6. The command report is sent back to the model with MSP command requests disabled.
7. The final answer is saved into the conversation.

## Translation Boundary

MSP commands do not call PowerShell, `cmd.exe`, Bash, or arbitrary host binaries. The current translation layer maps commands into controlled ReadOS services:

- `workspace`, `library`, and `pdf` commands translate to workspace/PDF services.
- `artifact` commands create durable `/artifacts/...` workspace files.
- `page-label` and `outline` commands mutate ReadOS document metadata through app services.
- `windows info` and `windows path ...` translate to safe .NET/Windows host metadata and local ReadOS paths.
- raw host filesystem access remains hidden behind virtual workspace paths and app services.

This is the first bridge between MSP core semantics and Windows desktop application APIs.

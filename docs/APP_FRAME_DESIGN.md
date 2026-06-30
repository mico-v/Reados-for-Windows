# App Frame Design

ReadOS currently uses a compact WinUI frame that should evolve from a reader layout into an MSP operator workbench. The document reader remains the first vertical domain, but the frame should make command execution, evidence, artifacts, and policy approval visible.

## Regions

### Top Toolbar

- toggle library/workspace pane
- import materials
- navigate current document pages
- jump to page number or mapped page label
- toggle thumbnails, outline, chat, transcript, and settings
- expose the current MSP session state

### Workspace Pane

- project/document navigation
- project creation
- file import
- search across projects and documents
- current document rename
- open file location
- delete current document into the ReadOS trash folder
- future `/artifacts` browser

### Evidence Surface

- rendered PDF page
- page navigation
- zoom-width slider
- page thumbnails
- editable page label
- outline list
- document text search
- red-box region selection layer
- future artifact preview surface

### Agent And Command Pane

- per-document conversation list
- persisted message stream
- page/range/region attachment queue
- prompt composer
- OpenAI-compatible send path
- offline response mode
- MSP command transcript
- command stdout/stderr, artifacts, and audit records

### Approval And Policy Surface

- show mutating command previews
- explain target paths and side effects
- allow, deny, or require confirmation from the transcript
- replay approved commands through one-shot host approval tokens
- keep a recoverable transcript of approved actions

### Settings Drawer

- language
- offline response toggle
- provider name
- base URL
- API key
- model name
- default prompts
- MinorU endpoint
- workspace export/import
- future policy and command-pack settings

## Data Ownership

The ViewModel owns UI state and command binding. Services own external effects:

- `WorkspaceStore`: JSON state, imported file copies, trash, export/import.
- `PdfDocumentService`: PDF render, metadata inspection, text extraction, and search.
- `FileDialogService`: WinUI file picker integration.
- `AiChatService`: OpenAI-compatible chat calls and offline fallback.
- `ReadOsMspHost`: app-owned MSP command execution boundary.
- `ReadOsVirtualWorkspace`: virtual MSP file projection over app state.

The window code-behind owns only window-specific behavior:

- dependency wiring
- pane resize thumbs
- root loaded initialization
- pointer handling for the red-box region overlay

## Current Design Constraints

- Keep the first screen as a usable workbench, not a marketing page.
- Keep document reading fast because it is the first MSP vertical domain.
- Make command transcripts and evidence inspectable without overwhelming reading.
- Keep controls dense and predictable for repeated work.
- Use local data by default and make cloud/model access optional.
- Do not commit workspace data, imported documents, artifacts, or API keys.

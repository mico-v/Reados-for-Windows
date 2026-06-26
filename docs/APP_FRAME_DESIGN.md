# App Frame Design

ReadOS currently uses a compact three-pane reader frame.

## Regions

### Top Toolbar

- toggle library pane
- import materials
- previous and next page
- jump to page number or mapped page label
- toggle thumbnails
- toggle outline
- toggle chat
- open settings

### Library Pane

- project/document navigation
- project creation
- file import
- search across projects and documents
- current document rename
- open file location
- delete current document into the ReadOS trash folder

### Reader Workspace

- rendered PDF page
- page navigation
- zoom-width slider
- page thumbnails
- editable page label
- automatic baseline page mapping
- outline list
- add/delete outline entries
- document text search
- red-box region selection layer

### Chat Pane

- per-document conversation list
- persisted message stream
- page/range/region attachment queue
- prompt composer
- OpenAI-compatible send path
- offline reading response mode

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

## Data Ownership

The ViewModel owns application state and commands. Services own external effects:

- `WorkspaceStore`: JSON state, imported file copies, trash, export/import.
- `PdfDocumentService`: PDF render, metadata inspection, text extraction, and search.
- `FileDialogService`: WinUI file picker integration.
- `AiChatService`: OpenAI-compatible chat calls and offline fallback.

The window code-behind owns only window-specific behavior:

- dependency wiring
- pane resize thumbs
- root loaded initialization
- pointer handling for the red-box region overlay

## Current Design Constraints

- Keep the first screen as the actual reader workspace.
- Avoid decorative page sections or marketing layout.
- Keep controls dense and predictable for repeated reading work.
- Use local data by default and make cloud/model access optional.
- Do not commit workspace data, imported PDFs, or API keys.

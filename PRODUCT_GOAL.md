# ReadOS Product Goal

ReadOS is a macOS-first AI PDF reading application designed for serious, long-term study. Its goal is not to place a chat box next to a PDF. The product should feel like a native, quiet, durable reading workspace where file management, PDF navigation, page metadata, AI questions, chapter-level explanation, review, export, and sync all belong to one coherent learning workflow.

## Product Positioning

ReadOS should be a native-feeling PDF reader first, with AI integrated into the act of reading instead of bolted on afterward. The target user is a heavy PDF reader: students, researchers, developers, professionals, and self-learners who repeatedly read textbooks, papers, manuals, and scanned books, then return later to review what they asked and learned.

The core promise is:

- Keep the reading experience as clean and smooth as a native macOS reader.
- Remove the repetitive work of screenshotting, switching apps, pasting images, explaining context, and managing chat history.
- Turn PDF reading into a long-lived learning system with preserved page labels, outlines, prompts, attachments, conversations, exports, and eventually sync.

## Design Principles

1. Reader first: PDF reading, navigation, search, annotation, thumbnails, tabs, and immersion must be excellent before AI features matter.
2. Native mental model: file management should follow Finder conventions wherever possible, so macOS users can reuse their existing habits.
3. AI reduces workflow friction: AI features should compress multi-step manual workflows into one or two natural actions.
4. Metadata should be portable: generated book page labels and outlines should be written into PDF metadata when exported, so they work in other readers.
5. User-owned AI: users must be able to configure their own providers, API keys, base URLs, models, prompts, shortcuts, and model-specific behavior.
6. Long-term memory: conversations, page references, reading progress, prompts, outlines, page mappings, and parsed caches should persist per document.
7. Quiet interface: the UI should be modern, compact, adaptive, and hideable; it should support deep reading without visual pressure.

## Core Product Areas

### 1. Library And File System

The app has its own document library with a root directory and folders. It should match Finder-like behavior:

- Create folders.
- Import PDFs and folders.
- Drag files between folders and back to root.
- Rename with Enter.
- Open with double click or Command+O.
- Delete with Command+Delete.
- Undo and redo destructive file actions.
- Trash/recycle area with restore and clear.
- Create folder with Command+Shift+N.
- Search current directory for folder and document names.
- Manually reorder files and folders, including Command+Up and Command+Down.

This file system is not just storage. It is the long-term reading workspace.

### 2. Immersive PDF Reader

The reader should feel close to macOS Preview where possible:

- Smooth scrolling.
- Trackpad zoom.
- Search and jump.
- Basic annotations.
- Page thumbnails.
- Multiple PDF tabs.
- Command+W to close the active tab.
- Hideable library sidebar, top toolbar, chat panel, and window chrome where feasible.

The ideal reading state can collapse down to almost only the PDF page.

### 3. Book Page Mapping

Many PDFs, especially scanned books, have PDF page indexes that do not match printed book page numbers. ReadOS should support AI-assisted book page mapping:

- Use a vision-capable model to identify cover pages, front matter, copyright pages, roman numeral pages, table-of-contents pages, and main body page numbers.
- Display mapped labels such as "cover", "front matter", roman numerals, and real book page numbers instead of raw PDF indexes.
- Let users jump by book page label, such as page 37 or "cover".
- Let users manually edit mapping rules and fix AI mistakes.
- Preserve the mapping when exporting by writing PDF page label metadata.

This feature is foundational because later outline generation, page-range attachment, export, and review all depend on correct book page identity.

### 4. AI Outline Generation

For PDFs without useful bookmarks, ReadOS should generate a navigable outline:

- Use a vision-capable model to read table-of-contents pages.
- Generate multi-level items matching the book structure, such as chapters, sections, and subsections.
- Link outline entries to the correct mapped book pages.
- Support manual add, edit, delete, and child/sibling insertion.
- Write the generated outline into PDF metadata on export.

The outline and book page mapping should cooperate: once page mapping identifies TOC pages, outline generation can avoid asking the user to manually enter the TOC range.

### 5. AI Attachment Workflow

The app should make asking about PDF content feel native:

- Attach the current page to chat with a shortcut, demonstrated as Command+Shift+A.
- Attach selected pages from thumbnails.
- Attach page ranges by entering mapped page labels, including multiple disjoint ranges.
- Preview attached pages.
- Export attached page subsets as PDFs while preserving page labels where possible.
- Import extra attachments such as images, PDFs, Markdown, and other model-supported files.

If the user presses Enter with only attachments and no written question, the app should insert a configurable default prompt.

### 6. Region Selection Explanation

The app should support a "select region to explain" workflow:

- User triggers a shortcut or toolbar action.
- User draws a rectangle over a PDF region.
- The app automatically creates attachments containing the selected page plus surrounding context pages.
- The selected region is marked with a red rectangle.
- A configurable prompt is inserted, such as asking the model to explain the red-boxed content using the surrounding context.
- Users can edit attachments afterward by deleting pages or splitting/cropping pages.

This replaces the old workflow of taking several screenshots, switching apps, pasting them, and manually explaining which image matters.

### 7. Per-PDF Chat Warehouse

Each PDF owns a chat warehouse:

- Conversations are bound to the PDF.
- Each conversation can retain page references and attachments.
- Users can search past conversations.
- Users can create new conversations, including via Command+N.
- Users can reopen prior conversations and jump back to referenced pages.
- Users can export a full conversation as a long image and copy or save it.
- Each PDF can have its own system prompt.
- Chat controls should expose model, reasoning effort, answer detail, web/search mode where supported, and other provider-specific options.

This is the review layer: the app should remember what the user asked while reading a specific document.

### 8. MinorU Integration And Chapter Explanation

MinorU-style parsing is useful as a bridge, not as the main reading surface:

- Convert scanned/image PDFs into Markdown with bounding boxes and structural position data.
- Cache the parsed result per PDF.
- Optionally show a PDF/Markdown comparison view with linked navigation.
- Use cached positions to support precise chapter/section extraction.

The key workflow is "explain this chapter/section":

- User right-clicks an outline item.
- The app uses the outline and MinorU cache to crop exactly that section.
- It excludes preceding and following section content from the first and last pages.
- It attaches the cropped section and inserts a configurable chapter-explanation prompt.
- User can press Enter to ask the model.

This should work for small sections, not only whole chapters.

### 9. Standalone Chat Mode

ReadOS also includes a non-reading chat mode:

- Independent folder/group structure for chats.
- Conversations inside groups.
- Separate default system prompt.
- Keyboard shortcut to switch between reading mode and chat mode.
- Code blocks should render well, support collapse, and support copy.

This mode should share model configuration and chat UI quality with reading mode, while staying separate from PDF-bound chat warehouses.

### 10. Settings And Configuration

Settings are central because the product should not lock users into one AI provider:

- Configure multiple providers, base URLs, API keys, models, and capabilities.
- Support mainstream and custom providers, including OpenAI-compatible endpoints.
- Choose models per task where useful: vision model for page mapping and outlines, stronger reasoning/chat model for explanations.
- Configure default prompts for attachment-only questions, region explanation, and chapter explanation.
- Configure per-PDF system prompts.
- Configure shortcuts.
- Configure MinorU API key or self-hosted endpoint.
- Configure export/import and future sync options.

### 11. Export, Import, And Sync

The app should treat user data as durable and portable:

- Manual export/import of the whole workspace.
- Include PDFs, folder structure, reading progress, page mappings, outlines, prompts, chat histories, attachments where appropriate, and MinorU caches where feasible.
- Export individual PDFs with embedded page labels and outlines.
- Future automatic sync targets include iCloud and PCloud-like providers.

Local data should remain usable without cloud services.

## UI And Interaction Standard

The app should follow macOS-native aesthetics:

- Use native components where possible.
- Prefer compact toolbars and adaptive controls.
- Avoid heavy PDF-editor-style tool clutter.
- Allow library, toolbar, chat, and side panels to be hidden.
- Support modern glass/translucent materials where the OS supports them.
- Controls should adapt as available width changes, with smooth transitions and tooltips.
- Reading should feel calm enough for long sessions.

## Development Priorities

### MVP Foundation

- App shell and persistent local data model.
- Library file system with folders, import, drag/move, rename, delete, trash, undo/redo, search, and ordering.
- PDF reading with tabs, thumbnails, page jump, search, and basic hideable panels.
- Settings framework for model providers, prompts, and shortcuts.

### AI Reading Workflow

- Per-PDF chat warehouse.
- Configurable model calls.
- Attach current page, selected pages, and page ranges.
- Attachment preview, export, and simple page deletion.
- Default prompt insertion.
- Conversation search and long-image export.

### Smart PDF Metadata

- AI book page mapping with manual correction.
- PDF page label writing on export.
- AI outline generation with manual correction.
- PDF outline writing on export.

### Precision Study Workflow

- Region selection explanation with context pages, red box, and editable attachments.
- Page splitting/cropping editor for attachments.
- MinorU parse/cache integration.
- Outline-item chapter/section explanation using parsed bounding boxes.

### Later Expansion

- Full workspace export/import polish.
- iCloud or PCloud-style automatic sync.
- Chapter video/audio explanation generation.
- Non-macOS versions if the native macOS app reaches product maturity.

## Non-Goals

- Do not build a generic PDF editor full of heavy tool palettes.
- Do not make AI features dependent on one proprietary model provider.
- Do not make cloud sync mandatory.
- Do not treat generated page mappings and outlines as app-only data when PDF metadata can preserve them.
- Do not optimize for one-off casual PDF opening at the expense of long-term study, review, and accumulation.


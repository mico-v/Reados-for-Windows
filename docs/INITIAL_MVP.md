# Initial MVP

This document records the initial MVP target and its current status.

## Original Goal

The first implementation pass was intended to prove the ReadOS interaction model:

- left navigation rail
- project list
- project document list
- import material command
- central reading workspace
- bottom/right-side composer
- settings drawer
- local state for projects, documents, activity, prompts, and conversations

That shell has now been replaced by a usable local reader workspace.

## Current Delivered MVP

The app now supports:

- persistent local workspace data under `%LOCALAPPDATA%\ReadOS`
- project creation
- importing PDF, Markdown, and text files
- copying imported files into the ReadOS workspace
- PDF page rendering
- page navigation and jump by page number or mapped label
- thumbnails
- document search
- existing PDF bookmark import
- editable page labels
- editable outline entries
- baseline automatic page mapping
- heuristic outline generation
- per-document conversations
- page, range, and region attachments
- OpenAI-compatible chat requests
- offline reading mode when no API key is configured
- settings persistence
- workspace export/import

## Acceptance Status

- App launches as a WinUI desktop app: complete.
- Main window uses a three-region reading layout: complete.
- Library panel shows real imported files: complete.
- Reader area renders selected PDFs: complete.
- Chat panel persists per-document conversations: complete.
- Toolbar contains import, page attach, region explain, outline, page mapping, settings, and panel toggles: complete.
- Panels can be collapsed or resized: complete.
- ViewModels own UI state; code-behind handles only window, picker, resize, and pointer-surface duties: complete.
- No secrets, API keys, or local user data are committed: complete.

## Follow-Up Integration Order

1. Add tests around workspace serialization and PDF metadata parsing.
2. Add drag/drop folders, restore UI, undo/redo, and manual ordering controls.
3. Add PDF metadata export for page labels and outlines.
4. Add region crop image generation and attachment editing.
5. Add vision model workflows for scanned-book page mapping and TOC extraction.
6. Add MinorU parse/cache integration.
7. Add packaging and installer flow.

# ReadOS App Frame Design

## Goal

ReadOS should use a compact Codex-like application frame instead of a dense PDF-tool dashboard. The first screen is a project/session workspace:

- Left rail: global actions, projects, sessions, settings.
- Main surface: active session stream.
- Bottom composer: request input and quick actions.
- Settings: focused overlay, not a permanent panel.

The MVP should make the product feel like a durable study workspace where projects collect materials and sessions hold reading/AI work.

## Layout

```text
+---------------------------------------------------------------+
| title bar: back, forward, sync, menu, window controls          |
+----------------------+----------------------------------------+
| left navigation      | active session header                  |
|                      +----------------------------------------+
| global actions       | session stream                         |
| - new session        | - command/result cards                 |
| - search             | - imported materials                   |
| - import material    | - notes and mock answers               |
|                      |                                        |
| projects             |                                        |
| - project            |                                        |
|   - sessions         |                                        |
|                      +----------------------------------------+
| settings             | composer                               |
+----------------------+----------------------------------------+
```

## Visual Direction

- Dark, compact, quiet.
- One left rail, one content column.
- No nested cards inside cards.
- Cards only for repeated stream items, imported materials, and status summaries.
- Use 8px or smaller corner radii.
- Use restrained borders and spacing.
- Keep command labels short.
- Default language is Chinese.

## Navigation Model

### Project

A project is the durable container for study work.

Project contains:

- name
- description
- materials
- sessions
- last updated label

Primary actions:

- create project
- import material
- start session
- select project

### Session

A session is a focused reading/chat timeline within a project.

Session contains:

- title
- relative time
- activity stream
- draft prompt

Primary actions:

- create session
- select session
- send mock prompt
- attach/import material into the session

### Material

Material is a PDF, Markdown, note, or other imported source.

MVP material actions:

- import mock material
- show material card
- open method placeholder

## Main Surface

The main surface should not try to render real PDFs yet. It should show the future workflow:

- Setup card: current branch/build command or project/session guidance.
- Material card: imported files for the active project.
- Activity card: mock edits, mock AI answer, or imported material summary.
- Composer: prompt input, plus/import controls, model/status affordance, send button.

## Settings

Settings opens as an overlay panel.

Settings MVP fields:

- language
- provider name
- base URL
- API key
- model
- default attachment prompt
- region explanation prompt
- chapter explanation prompt
- MinorU endpoint
- use mock responses

Settings should not consume the main layout permanently.

## MVP Acceptance

- App launches into the compact project/session frame.
- Left rail can create projects.
- Left rail can start sessions.
- Left rail can import material into the active project.
- Selecting a project updates the project/session context.
- Selecting a session updates the main stream.
- Composer sends a mock prompt into the active session.
- Settings opens and edits app configuration fields.
- UI defaults to Chinese and can switch to English.
- Build remains clean with `.\scripts\run.ps1 -BuildOnly`.


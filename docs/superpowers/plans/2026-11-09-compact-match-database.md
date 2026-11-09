# Compact desktop match database

**Goal:** Use the available desktop width for a compact match table and remove the separate match board preview.

**Design:** Keep the database sidebar, search/filter controls and batch management. Give the table all remaining width and remove the previous 1800px application-body cap specifically for the database tab. Use single-line rows with modest vertical padding, aligned numeric columns, inline custom names and notes. Separate file format/position count from player names so ordinary entries do not occupy multiple lines. Preserve full text in accessible content/tooltips and the existing metadata editor. Keep analysis board/candidate previews, position browser and theme previews unchanged.

## Tasks

- [x] Add browser coverage at 1800×1169, 2560×1440 and 3840×2160: no match preview, table uses the content width, ordinary and long entries stay compact, all actions remain reachable, and selecting a row triggers no preview requests. Observe a failing test before implementation.
- [x] Remove `MatchPreview` and its state/props/styles from `MatchDatabase.tsx`, `App.tsx` and `library.css`. Keep keyboard row navigation and opening via Enter, double click or player button.
- [x] Provide a direct, labelled metadata edit action per row. Keep SGF export, bulk actions, tags/comments, custom display names, status, PR and all existing match columns.
- [x] Update existing database browser coverage to use the direct edit action and add keyboard, long-content, metadata and export checks.
- [x] Check frontend tests, TypeScript/Vite, formatting and affected browser workflows using a disposable PostgreSQL database/backend. Inspect screenshots on the intended desktop sizes, then build the served frontend and verify the main library read-only. Preserve the uncommitted 4-ply work and private demonstration data.

No database/API change or commit is required.

Verification: 108 frontend tests, TypeScript/Vite build, Prettier and 62 affected browser regression tests passed. The two unchanged native depth browser scenarios were excluded from this UI-only regression run; the preceding 4-ply task already verified them. New checks cover compact ordinary/long rows, viewport-width use at 1800/2560/3840, usable notes widths (including a 1440px desktop), no preview requests, keyboard focus, direct metadata editing, Enter/double-click opening, export and batch actions.

Read-only checks against the main demo library measured normal rows at 35px (previously mostly 79px), with table widths 1517/2277/3557px and no page-level horizontal overflow. Saved settings and database/completed-job fingerprints remained unchanged. The disposable backend/database were removed; private screenshots stay outside Git.

## Unified row hover summary follow-up

The user requested one hover text per row. The row now owns one multiline native title; all child-cell/button titles were removed while action aria-labels remain. A localized formatter provides ID/custom name, players, score/length, available PR/provenance, games/positions, dates, tournament/round/place and bounded tags/comment excerpts. Summary maps are memoized by match records and locale so resize renders do not reformat long notes. No API/database or layout change is involved.

Verification: 122 frontend tests (including nine summary cases across all six locales), eight affected desktop/resize browser tests, TypeScript/Vite build and formatting checks passed. Regression tests first failed on nested titles and then verified one row title with per-match content and no descendant titles.

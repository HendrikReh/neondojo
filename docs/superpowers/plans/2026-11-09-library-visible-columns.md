# Configurable match-table columns

**Goal:** Let the user choose visible Match database columns, with Tournament and Comment hidden by default and Tags independently visible.

**Design:** Add a compact Columns picker to the existing table toolbar. Persist visibility locally, separately from widths. Keep selection and action columns available; require at least one data column. Split the current Tags/Comment column into independent Tags and Comment columns. Preserve previous widths by migrating the old notes width to Tags and providing a default width for Comment. Resize hidden columns independently and restore their widths on re-showing. Column visibility is presentation-only: filters, ordering, current rows and batch selection stay intact; a hidden active sort is named in the toolbar. If White is hidden, Black keeps the existing match-open button; when both are hidden, Actions offers opening the match. Preserve Enter, double-click, position-filtered opening, exports, metadata and row summaries.

Files: library-column-preferences.ts (validation/storage/defaults); library-columns.ts (width layout migration); useLibraryColumns.tsx (visibility + resize state); LibraryColumnPicker.tsx (accessible chooser); MatchDatabase.tsx and library.css; six-language library messages; focused unit/browser tests and updates to prior tests that explicitly exercise optional columns.

- [x] Add failing default/visibility/migration tests and a real-browser default-column test.
- [x] Implement visibility model/picker, separate Tags/Comment, conditional headers/cells/colgroup and opening fallbacks; preserve width profiles.
- [x] Verify frontend tests/build/format and database/sorting/resize/position-search browser flows on a disposable database; review.
- [x] Activate static assets without restarting the main backend, verify read-only against existing matches, and clean disposable resources. No commit, backend/schema changes or private/demo data in Git.

TDD evidence: the original default table still exposed Tournament and had no independently configurable Tags column; the old notes-width profile did not provide tags/comment widths. New unit/browser tests reproduced both. The implemented defaults, persistence, corruption handling, independent hidden widths and legacy migration pass. All 207 frontend tests, the production build and formatting pass.

All 27 relevant database/sorting/pagination/position-search/import/analysis browser regressions pass. Review found a concrete edge case when both player columns were hidden with Actions fixed to 88px: controls needed 82px in a 72px content area. A new browser width assertion reproduced that failure. In this compact configuration only, export now shows a download arrow while retaining its accessible SGF name, keeping Open/Edit/Export reachable without changing width profiles. The final 11 visibility/resize/desktop browser tests pass again, including this assertion. Read-only review found no other actionable issues.

Activated the verified static bundle without restarting the main backend. A read-only smoke test on an existing match verified default optional columns, independent Tags/Comment, persistence after reload, default reset, retained selection, opening controls and containment at 1800×1169, 2560×1440 and 3840×2160. No write requests occurred and application settings were unchanged. Removed the disposable test service/database and temporary bundles; no backend/schema changes, commits or private/demo files were added.

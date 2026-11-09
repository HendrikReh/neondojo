# Typography consistency across four workspaces

Scope: review and fix font family, size and style inconsistencies in Database, Analysis, Positions and Study & Anki, including shared controls/dialogs. Preserve current navigation, density, themes, board artwork and all uncommitted features. No commit requested.

Rendered baseline audit at 1800×1169 (artifacts outside Git): Analysis title Georgia25px/550; Database Georgia30px/550; Positions Georgia27px/550; Study sans-serif21px/550. Ordinary controls vary 11–14px. Database/Positions table headers 11px but differing550/650 weights; Study headers13px/700. Settings title Georgia28px/400. Note textareas already inherit sans-serif; specialist position IDs deliberately use monospace. Platform-font inspection confirms Georgia for titles versus SF NS for UI.

Shared role scale: page title24/600, panel title14/600, preview subject16/600, dialog title20/600, study question18/600; ordinary body/control13 (body400, controls500), labels12/500, table12/400, table header11/600, secondary11/400, micro10. Retain existing UI sans-serif stack as one shared font token; monospace for code/IDs and original board/brand artwork are intentional exceptions. No rule requires all text to have the same size. Weights use400/500/600/700. Avoid external font downloads.

- [x] Record computed styles and screenshots with real data in all four views; trace actual font sources before changing CSS.
- [x] Add browser contract regressions, demonstrate baseline title/control/table failures on a disposable database.
- [x] Define shared typography tokens and apply them to existing heading/control/table rules; remove unintended serif/italic UI overrides and normalize equivalent weights. Make Study page title an h1. Preserve icon/board/brand/code exceptions and stable row/control geometry.
- [x] Verify computed family, sizes, weights, styles, table headers/rows, textareas, preview/study/dialog roles, six theme modes and six locales at 1800/2560/3840. Run frontend units, build/format and relevant workflow regressions; independent review and visual QA.
- [x] Document the role scale and verified findings. Activate frontend only after checks; read-only main smoke, unchanged backend/settings/data, clean task-owned temporary resources. No private/demo data or audit screenshots in Git.

Verification evidence (artifacts outside Git):

- Baseline computed-family regression failed against the original Georgia Analysis title. Corrected the audit assumption about note editors: their family already inherited the UI font.
- Frontend: 281 tests in 48 files passed; TypeScript/Vite production build and formatting passed. Vite retains its existing large-chunk warning.
- Broad browser regression: 131 selected tests passed; the two costly 3/4-Ply engine execution cases were excluded because engine behavior is unchanged.
- After final visual corrections: 20 focused browser tests passed, including all nine typography scenarios, desktop panes, typed error actions and settings. Matrix word integrity and shared column geometry are checked for all six languages at all three desktop sizes. Multiword labels may wrap at word boundaries.
- Source review found no remaining actionable issue after fixes. Real-data screenshot audit checked all four workspaces, preview boards, settings and light/dark modes at the desktop sizes. CDP confirms SF system fonts for the main title. No external fonts or user/demo data added.
- Final CSS normalizes ordinary toolbar actions and dialog titles, preserves code/board fonts, and shares statistics grid tracks so headers align with numeric cells. Detailed per-game statistics scroll inside their pane when their content needs more width.

Main activation verified at port 8787 with no write requests or browser exceptions. Backend PID, stored settings and application counts matched the pre-activation snapshot. Only this task’s disposable database, test service and temporary bundles were removed.

# Hint modal and checker preview

Goal: Hint opens a separate, dismissible modal; concrete checker suggestions can be explored on a read-only preview board without changing the saved or unconfirmed live turn.

- [x] Add failing browser coverage for Hint from a partial turn, both colors/themes, candidate selection and Initial/Arrows/Final views, Cube/pre-roll return, pending load/error and unavailable previews.
- [x] Extend PlayBoardDialog with optional dismiss semantics (Close/Escape, ordinary dialog role); retain required-action semantics, dragging and light backdrop for other dialogs.
- [x] Reuse DecisionDetails with optional candidate selection controls and existing candidate keyboard navigation. Add PlayHintView for the read-only board and existing projectMove validation, view controls and mode-filtered details. Cube alternatives stay textual.
- [x] PlayWorkspace routes Hint to one modal at a time, preserves partial steps and dice order, disables live interactions and suppresses duplicate inline hints, restores underlying Cube/pre-roll/board on close, and clears local view across workspace/session changes. Loading and errors stay in the modal; no stale suggestions after failure.
- [x] Add all six locale strings and responsive styling; update existing hint tests to close the view before continuing live interaction.
- [x] Build, unit tests, focused browser regressions, desktop/mobile screenshots, review and documentation. Mock API browser checks leave the user's matches unchanged.

Verification: observed initial failing modal test and focus-restoration regression before implementation/fix. Production build passed; 330 unit tests and 57 focused browser tests against the built app8787 passed. Desktop and mobile screenshots inspected. Review found the removed-opener focus issue, now fixed and covered, with a clean follow-up review. Hint-specific retry preserves partial steps; explicit saved-state reload remains a separate recovery action. Formatting checks passed; mocked API browser fixtures did not alter the user's matches.

# Saved study report in analysis header

Approved design: show a labelled Spielauswertung action beside the match name only when a saved report exists; open the existing draggable dialog without navigating or recomputing.

- [x] Add failing backend tests for metadata availability (finished report vs unfinished/missing report, unrelated matches); add browser cases for header action, lazy reads, exact report, unchanged analysis position, errors/retry and stale navigation.
- [x] Expose nullable study_report_id in DatabaseMatch using the existing play_sessions join and the same report predicate as Study. Regenerate OpenAPI/types, keeping legacy frontend mocks compatible.
- [x] Add an analysis report action component with lazy abortable loading, identity checks, visible-only modal, retry and focus return. Place its labelled action directly after the header title, passing the current board theme and analysis visibility.
- [x] Verify relevant backend and frontend tests, header/dialog/browser regressions and responsive layout, review scoped implementation, update documentation and local server.

Validation: all 386 frontend tests, 26 PostgreSQL study/database tests, production build, 25 browser/header/dialog regressions and scoped independent review passed. Native read-only smoke check opened the real saved report for match 34 (Hendrik vs. GNUbg) via the German Spielauswertung action, preserved the position, returned focus on Escape, and recorded no study/play/job writes. Responsive screenshots inspected at 1440 and 390 px. Local backend and frontend refreshed.

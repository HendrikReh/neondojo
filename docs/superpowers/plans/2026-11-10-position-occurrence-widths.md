# Position search column widths

Goal: Database/game/move headers use the existing resize handles and persist their widths in library.column_widths, including profile restore and reset.

- [x] Add failing frontend/backend profile tests for the three width keys, bounds and backwards compatibility; add browser resize, keyboard/cancel, reload and profile-storage checks.
- [x] Keep ordinary column visibility/order definitions unchanged. Add occurrence width specs and a combined resize layout with legacy-width migration. Activate occurrence columns only in position-filtered results.
- [x] Use the shared resize hook for all rendered columns, replace fixed widths and additive table sizing with combined sizing, and wire resize handles to the three headers.
- [x] Extend profile width validation only; retain existing profile/order schema. Verify backend configuration tests, frontend suite/build, browser table/search regressions, scoped review and update the local server.

Verified: all 382 frontend tests, 12 PostgreSQL configuration tests, 23 browser search/width/visibility regressions, production build, scoped independent review and local server HTTP check. Browser tests inspect canonical profile state rather than the former local-storage cache. Tests cover dragging, keyboard increments, Escape, reset, reload, search-context toggling and preservation of ordinary column widths. Server refreshed with updated profile width validation.

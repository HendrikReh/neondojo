# Direct search from the analysis board

User requested on 2026-11-10: “Diese Stellung suchen” immediately runs exact library search for the current source position. Retain the chosen database, other filters, and sorting.

- [x] Update the existing action E2E to require a filtered request and populated search field after one click, without manually using/submitting the position form. Verify the pre-change failure.
- [x] Send a tokenized requested-position-search intent from App to MatchDatabase. Consume each intent once to open the position form, set the source position filter, clear input errors and trigger existing query/cursor/selection cancellation behavior. Repeated explicit clicks must rerun search; plain tab navigation must not restore an old intent.
- [x] Verify repeated searches, changing source positions and late-response exclusion, preserved database/other filters, existing manual position search and bookmark behavior. Run frontend build/tests and focused browser checks, then document final behavior.


Verification on 2026-11-10:

- One-click integration failed before implementation (no visible populated search field or filtered request), then passed without manual position transfer or form submission.
- Six action cases pass, covering desktop/mobile placement and tooltip, direct source-position search even in Final view, bookmark-load failure independence, database/other-filter preservation, explicit same-position refresh with cleared selection, no replay after clearing the filter, and rejection of a delayed previous position result.
- Nine existing manual-library-position and saved-position browser cases pass. Total: 15 focused browser cases against the updated local production app.
- All 57 Vitest files / 341 frontend tests pass. TypeScript/Vite production build passes; existing bundle-size advisory remains. Changed files pass Prettier and git diff --check.
- Existing backend and search APIs are reused, with no migration, engine work or server restart. Updated production frontend is already served locally. Unrelated uncommitted work and the existing feature branch are preserved.


Scope policy superseded by the user's subsequent instruction on 2026-11-10: the analysis action now selects All databases and clears every other filter. See 2026-11-10-global-position-search.md for implementation and verification; manual library search retains its scoped behavior.

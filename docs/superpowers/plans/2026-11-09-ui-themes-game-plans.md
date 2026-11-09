# UI themes and two-player game plans

**Approved scope:** Implement the previously proposed Graphite, Club and Paper interface themes, each with Light/Dark/System appearance; implement two-player game-plan classification beside the analysis board. Audit the remaining discussed features and report their status. Do not silently expand this request to deck CRUD or checker/cube statistic breakdowns. Preserve the existing dirty repository, board themes, imports, analyses, database administration and learning. No commit requested.

**Architecture:** UI colors use shared semantic CSS tokens across every app view, dialog and control, with stable semantic error colors. Theme and appearance are local UI preferences, independent of the BGBlitz/board theme. Settings provide previews and keep save/cancel semantics; System follows matchMedia changes and stored appearance applies before first paint.

Game-plan classification is a small deterministic Rust module derived from a pinned public Open Sage reference (with source/license attribution), not a Python runtime or a switch away from GNUbg. `POST /api/game-plans` accepts `{board:[26 signed absolute-color point counts]}` and returns `{classifier:"opensage_rules_v1",source_revision:string,game_over:boolean,white:GamePlan,black:GamePlan}`; GamePlan is `pure_race|racing|attacking|priming|anchoring`. Validate board counts/signs, preserve White/Black identity and correctly adapt bar signs/perspectives. This board-only label is called Game plan, not a score-aware engine recommendation.

The client classifies exactly the board being displayed: Initial and Arrows use the initial board, Final uses the successful played-move preview's resulting board. A bounded board cache, abort/version fences and explicit request scope prevent late labels from replacing newer positions. Two compact named player labels share the board's locale and appearance. GNUbg and depth defaults stay unchanged.

- [x] Theme domain: tests first; implement tokens, six palettes, bootstrap/persistence/System behavior, settings controls/preview/save/cancel. Own App/Settings/CSS integration separately from strategy files.
- [x] Rust classifier domain: pin reference, parity fixtures and validation tests first; implement pure classifier, REST/OpenAPI and attribution. No database migration.
- [x] Game-plan client domain: test loading/current-board/late-response behavior; add API client, cache, localized compact panel and AnalysisWorkspace integration.
- [x] Integrate generated API types, inspect all views/dialogs in six theme modes, and verify board-theme independence, depth defaults and existing workflows on a disposable database.
- [x] Audit discussed features and maintain a concrete implemented/partial/planned inventory; review spec and code quality, address findings.
- [x] Build and activate after preserving main data/settings and checking idle jobs for backend restart; private backup, read-only smoke and disposable-resource cleanup. No private/demo data in Git.

Tasks are independently delegated with explicit file ownership using the parallel-agents workflow; root handles integration, verification, documentation and the status audit. No automatic commits or new checkout that would omit the current uncommitted features.

Verification so far: 260 frontend tests, 92 backend unit tests (26 deliberately ignored) and 10 game-plan rule/API tests passed. Pinned C++ parity independently reviewed for 18,743 positions and 33 fixtures, both colors/reflections. Three scoped read-only code reviews completed; two CSS regressions (progress fill and invalid path outline) identified and corrected for browser recheck. Integrated production builds, formatting and API type generation passed. Browser and activation checks completed below.

Final verification:

- Production Rust/frontend builds, generated REST/OpenAPI types, whole-web Prettier, cargo fmt and git diff whitespace checks passed.
- New browser coverage: 13/13 passed, including all six palettes, independent original ZIP board artwork, System changes, Save/Cancel/conflict, exact visible-board requests, stale-response ABA rejection and retry/terminal state.
- Full regression run: 105/106 passed initially; the one old hover-color assertion was updated to the chosen theme token and its focused rerun passed (1/1). No runtime regression remained. The two existing expensive 3/4-ply engine browser cases were excluded; their profiles/validation remain covered by settings/unit tests and the previous 4-ply implementation.
- Inspected desktop screenshots for Analysis, Match database, Positions, Study and Settings across six appearances; density/containment verified at 1800×1169, 2560×1440 and 3840×2160. Progress fill was visually reproduced blank before the CSS correction and distinctly filled afterward.
- Activated on localhost:8787 after an idle-job check and fresh private PostgreSQL backup; no schema migration. Six appearances and Initial/Final/Arrows/stepping work against existing main data. Read-only browser guard recorded no unexpected writes or page errors; only the pure game-plan endpoint was allowed to POST.
- Main settings exactly unchanged, including 2-ply default. Counts unchanged: 33 matches, 47,936 positions, 2 saved positions, 71 learning cards, 65 decks, 72 deck memberships, 64 reviews and 25 jobs. One user-started analysis completed before the final backup/restart; it was preserved.
- Disposable test service/database and staging/rollback bundles removed. Latest private backup retained outside Git. No commit created.

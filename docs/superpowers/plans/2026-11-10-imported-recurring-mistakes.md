# Imported recurring mistakes implementation plan

**Goal:** Include immutable XG/BGBlitz source analyses in recurring groups and hidden-answer practice while preserving provenance and preventing duplicate counts.

**Architecture:** A pure source projection module validates imported records against original source moves. The existing bounded loader merges imported-only games with selected native games; stable typed references identify each source. UI uses original depth labels rather than inventing comparable ply values.

- [x] Pure projection: create `backend/src/recurring_mistakes/imported_projection.rs` with tests first. Shared contract: `Projection { decision: Decision, position: Position, depth: Option<String>, depth_plies: Option<i32> }`; `project(record: &ImportedAnalysisData, source: &SourceMove, match_id: Uuid, position_uuid: Option<Uuid>, source_format: &str) -> Option<Projection>`. Validate source IDs and legal actual/best boards, normalized XG/BGBlitz EMG loss, cube directions/perspectives and unknown/sentinel units. Share the existing `move_assessments::after_play` resolver by widening only its visibility.
- [x] Loader/API: extend `source.rs` with source catalogs, bounded imported loading, whole-game native precedence and unified answer resolution. Add optional imported reference UUID and provenance fields in `recurring_mistakes.rs`; preserve native request compatibility and privacy. Write failing API tests, then implement and run them against disposable PostgreSQL databases.
- [x] UI: update generated types, imported reference keys, depth/method/source display, localized coverage wording, unknown-depth fallback and browser tests. Keep every answer field behind Reveal.
- [x] Integrate and review: run `npm run api:types --prefix web`; check format/Clippy, Rust suite and targeted PostgreSQL/native/import tests, full web unit/build and recurring/Study browser workflows. Verify live source groups and hidden/revealed imported questions. Update README/navigation and record exact verification evidence.

Independent owners: source projection agent owns only its new module and notation-resolver visibility; loader/API agent owns backend integration/tests; parent owns frontend/docs/schema generation and final integration. All work remains uncommitted in the existing tree.

## Verification evidence

- Projection tests: 15 passed, including real XG binary and BGF parser output; malformed/sentinel/MWC-only cases excluded.
- Rust workspace suite: 169 passed, 288 intentionally ignored integration/engine cases.
- Combined PostgreSQL/native/source/search/retention integration: 68 passed. After the native artifact-dedup fix, all 17 affected recurring/imported/native tests passed, including the new three-game/22MiB regression.
- Web unit suite: 456 passed across 70 files.
- Browser workflows: 17 passed (import provenance and answer reset plus all existing recurring/Study checks).
- Formatting, strict Clippy, OpenAPI/type generation, TypeScript/Vite build and diff whitespace checks passed. Existing Vite warnings remain.
- Running backend refreshed after confirming no queued/running analyses. Live XG and BGBlitz source practices and reveal references verified without creating engine jobs. Native whole-game precedence retained.
- Live large-player smoke: 61,731 analyzed decisions, 22 groups, five matches. The cold debug-server request took 19.236s; typical single-match examples took about 0.3–0.7s.
- Fixed the native loader counting/transferring the same source once per checkpoint game: source bytes and SGF payload now occur once per match while all game contexts remain.

All changes remain uncommitted; pre-existing work preserved.

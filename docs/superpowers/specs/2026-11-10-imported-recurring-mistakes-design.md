# Imported XG and BGBlitz recurring mistakes

The user requested inclusion of imported XG and BGBlitz analyses in the existing recurring-mistakes dashboard and one-click practice. This extends the approved feature without changing decks or FSRS history. Work in the existing dirty tree, preserving unrelated changes and making no commits.

## Selection and presentation

For each match/game, retain the existing highest-depth/newest completed current native-job choice. If such a checkpointed game exists, use it exclusively, even if its decisions are unavailable or below threshold. Otherwise use the immutable imported analysis rows for XG/BGF sources. Never rank foreign ply numbers against GNUbg ply numbers. Count each source occurrence once, group by existing game-plan/kind dimensions, and preserve actual deciding players. Foreign missing, malformed, incomparable or unranked played moves remain unknown. Exclude removed sources and protected competition sessions as before.

Include eligible imported-match players in choices without parsing entire libraries. Lazily catalog legacy XG/BGF imports with bounded work when selected; do not silently return partial coverage. Keep source selection/load bounds and source visibility checks. Show native/XG/BGBlitz source, original engine/version, method and depth on practice questions/answers; unknown depth remains unknown, not zero-ply. Explain local-first inclusion in all six UI languages.

## Foreign assessment

Join records to validated `parse_sgf_source_moves` by exact match, game and source move, using its persisted `match_moves` catalog for current imports. Lazily create missing legacy catalogs in bounded steps. This avoids reparsing over 100 MB of SGF for existing XG bulk libraries. Validate kind, deciding player and checker ply; use the original source board/IDs/score/cube/dice. Checker alternatives must replay legally to final boards with the existing notation resolver, including grouped/reordered moves. Require a unique best and a matching actually played candidate. XG cubeful equities yield normalized loss directly. BGBlitz explicit `hasEMG`/`emg` candidate values yield normalized loss even when displayed candidate equities are MWC. Never treat an MWC fraction or unknown unit as normalized equity, invent conversion or infer a missing candidate's loss. Preserve projected source data unchanged; assessment is a read-only derivation.

XG cube options are from the doubler's perspective: offer chooses ND versus min(DT,DP), responder chooses max(-DT,-DP). BGBlitz raw cube options are from the current source actor's perspective, with explicit `useMatchEquity`; source `pr.cubeError` is the normalized signed error while `cubeErrorMWP` is the MWC counterpart. Reject sentinel -99 and missing/inconsistent numbers. Offer/response best actions follow options in the correct perspective. Use only demonstrated normalized source losses. Reconstruct pre-roll no-double and outstanding offers correctly, without exposing the following roll. Cube responses always belong to the responder.

## API compatibility

Extend existing `MistakeReference` with optional `imported_analysis_id`, and make `job_id` optional; exactly one must be supplied. Keep existing native requests valid. Imported references use immutable imported row UUID plus game/source-move number in the existing `game_no`/`decision_no` fields. Practice reveal resolves and revalidates that row's current eligibility and local precedence; stale or forged references fail. Serialize absent optional IDs as omitted to preserve old native wire responses and tests.

Make `plies` nullable in question/answer, and add optional `depth`, `method`, and `source` (`native|xg|bgblitz`) fields. Native returns its actual depth; foreign returns its original labels such as XGRoller++ or rollout trials. Answers stay solely behind Reveal. Update frontend question keys to distinguish imported references. All old answer-hiding, cancellation, context and focus behavior remains.

## Validation

Write failing pure projection tests and isolated PostgreSQL API tests before implementation. Cover legal checker notation and played-candidate presence, XG normalized loss and all cube directions for both colors, BGBlitz EMG/MWC distinction and sentinel rejection, source identity/context validation, source engine/depth preservation, native whole-game precedence before thresholds, legacy catalog, source removal/competition exclusions, reference validation and hidden answers. Exercise real binary XG and BGF parser output as well as small synthetic records; no foreign result becomes a GNUbg job. Regenerate OpenAPI/types; run existing and extended recurring/native/search/profile tests, web tests/build and targeted browser flows. Review source and verify the updated running backend with the local library.

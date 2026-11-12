# Candidate comparison and imported decision review

Approved scope: implement the candidate comparison and error navigation proposed
in the UX review. German board controls remain Initial / Pfeile / Final. Remove
the BGBlitz link below the board. Preserve existing unrelated working changes.

## Design

Candidate tables share a compact numeric comparison: move, played/best/preview
identity, equity, comparable difference to the best result, and win probability
with one decimal place. Preserve engine, units, perspective and differing depths.
Unavailable or incompatible evaluations remain unavailable, never zero.

Error review uses server-validated decisions with per-row provenance, including
imported results without requiring a local job. Retain filters, pagination,
checkpoint ownership, and stale-response cancellation. Native PR remains native.
The selected decision and the board must remain consistent.

## Implementation and verification

- [x] Backend: reuse existing imported decision projection and native checkpoint
  selection; expose bounded review pages with source metadata. Add isolated
  integration coverage for imported-only, partial native coverage, filters,
  missing/invalid units and cursor consistency. Regenerate OpenAPI/TS types.
- [x] Candidate UI: share table rendering across native/imported/book candidates;
  retain click/keyboard preview and distinguish best, played and displayed moves.
  Add comparison edge-case tests and browser coverage.
- [x] Error UI: use review pages in the error table and Next Error; preserve
  scope, filters, wrapping and request cancellation. Add imported-only and
  delayed-response browser regressions before implementation.
- [x] Polish: German Pfeile label; remove board footer link; update README.
- [x] Run focused tests, frontend unit/build, Rust fmt/clippy/tests and isolated
  integration checks appropriate to the backend changes. Visually inspect the
  desktop views at 1800×1169, 2560×1440 and 3840×2160.
- [x] Review the final diff for correctness, duplicated rules and unrelated edits.

No commit, push, server restart or publication is included in this request.

## Verification results

- Frontend: 75 unit-test files / 506 tests passed; production build passed.
- Browser: 29 affected scenarios passed; the two imported-review scenarios passed
  again after the final cube-context restriction. Includes an actual WC2014.xg
  import, candidate interaction, filter scope, delayed navigation, wraparound,
  and cube pre-roll context. Screenshots inspected at all three target sizes.
- Backend: workspace tests passed; final changes additionally verified with
  8 review SQLx tests, 8 recurring-import SQLx tests and 15 projection unit tests.
  Rust formatting, Clippy with warnings denied, API generation and diff checks passed.
- Integration tests used disposable databases. Ignored live-engine suites were
  not additionally run; this change reads saved evaluations and checkpoints.
- Independent review findings fixed: book units, empty source candidates, cube
  navigation anchor/context, record switching, and SGF threshold rounding.
- SGF cube records without established units/perspective remain source-visible
  and excluded from combined error classification; documented in README.
- Build reported mixed-import/chunk-size warnings; no build errors.

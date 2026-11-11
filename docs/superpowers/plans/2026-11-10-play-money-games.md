# Money games in New Match

Goal: Add Money Game (match_length 0) to New Match, preserve the selection in profiles, and run one native GNUbg money game per session without a point target. Completing the game enables the existing saved learning review.

- [x] Add failing native creation/play/cube/completion/archive tests and frontend/profile validation and creation-flow tests for length zero; retain invalid bounds and ordinary point matches.
- [x] Allow 0..25 through session/profile validation. Native adapter uses new session for zero, never treats 0 as an achieved target, and removes point-target cube restrictions for money play. End the session only when the money game actually finishes.
- [x] Add localized Money Game option and readable money labels; avoid 0-point targets and incorrect target-covered cube/resignation explanations.
- [x] Verify native state restoration, export, scoring, library metadata and saved report, plus UI/profile tests, relevant existing play regressions, build and scoped review. Refresh local app only after tests settle.

Verified: 387 frontend tests, production/OpenAPI build, 32 native play regressions (long full-match simulation excluded), additional money Coach test, 13 PostgreSQL configuration tests and 26 browser creation/opening/resignation/archive cases passed. Both money-native tests were rerun after final changes; scoped independent review found no concrete issues. Native money creation, both colors, ordinary double, resignation award, single-game completion, export, library linkage and persisted review were exercised. An existing randomized library-detach test now handles native cube/pass phases instead of assuming every non-roll state is a checker move. Local server refreshed.

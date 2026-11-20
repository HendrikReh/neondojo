# Decision previews in the learning review

User requested on 2026-11-10: clicking a learning-review decision shows the matching board and best move, like Hint, in the existing shared modal for Playing and Study.

Use the existing read-only, versioned /play/sessions/{id}/history endpoint, which reconstructs stored snapshots without an engine and survives library deletion. Match all reviewed decisions (including opponent decisions) chronologically to snapshots; checker_no identifies checker turns, preceding no-double shares that starting board with dice hidden, and actual cube events are matched in chronological order. Fail closed on inconsistent or absent data. Never preview from the session’s final board.

- [x] Add failing pure mapping tests: repeated turns, both colors, game boundaries, no-double vs checker, multiple double/take sequences, missing/invalid data and no mutation.
- [x] Add failing browser coverage: row selection, correct best-move arrows and Initial/Final board, row keyboard navigation, saved competition/Study and Playing, cube preview, stale/error/missing data and viewport containment.
- [x] Implement learning-review-positions.ts mapping and a lazy version-fenced history hook. Only load snapshots after selection; abort on close and preserve immutable sessions.
- [x] Add read-only LearningDecisionPreview using Board, projectMove and the existing view controls; wire it to selectable rows in PlayLearningReview. Pass the actual board theme from both entry points. Keep summaries fixed and both list/preview reachable on narrow/short screens.
- [x] Translate labels, verify unit tests/build and focused report/Hint/dialog browsers, inspect desktop/mobile screenshots and actual saved report, review changes and update documentation.


Verification on 2026-11-10:

- Nine new pure mapping tests pass, covering checker ordinals across both players, repeated moves/game boundaries, no-double/shared board, repeated cube sequences, dice validation, forced pass, missing/invalid links and immutability.
- New browser test initially failed because decision preview was absent. Thirteen new preview cases pass for Study and Playing, native best-versus-played projections, Black coordinates, keyboard and cube choices, errors/retry, invalid best notation, version mismatch, rapid selection during load, closing pending reads, missing links and desktop/mobile containment.
- A review found updated mobile boards could remain above a long scrolled list. A 35-game reproduction failed with board viewport ratio0, then passed at >=0.9 for pointer and keyboard selection after the fix. A stacked-layout return action restores focus/scroll to the selected decision. Follow-up review found no actionable issues.
- All 57 Vitest files / 341 frontend tests pass. Production TypeScript/Vite build passes (existing bundle-size advisory remains). All 38 focused preview/report/Study/Hint browser cases pass. Formatting and git diff --check pass.
- Actual existing report verified in the local production app: 111 saved snapshots match 111 history events; selected checker1.15 uses original checker8 and its best double-one move. Source point counts were compared directly against the saved snapshot, and best-move arrows rendered. Desktop/mobile screenshots inspected at /tmp/neondojo-learning-decision-preview-desktop.png and /tmp/neondojo-learning-decision-preview-mobile.png. No API writes or runtime errors.
- No backend changes, engine analysis, migration or server restart required. Existing feature branch and unrelated uncommitted work preserved; no push or merge.

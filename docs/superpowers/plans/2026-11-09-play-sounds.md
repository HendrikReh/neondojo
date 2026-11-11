# Game Sounds Implementation Plan

**Goal:** Provide quiet sounds for moving, rolling, hitting and bearing off, with a saved mute switch.

**Architecture:** Pure event classification/replay in play-sound-events.ts, audio synthesis/playback/preferences in play-audio.ts; explicit interaction hooks in PlayWorkspace and the existing Settings save/cancel flow. Browser-only preferences follow theme conventions.

**Tech Stack:** React, TypeScript, Web Audio, Vitest, Playwright.

- [x] Write failing unit cases for per-step color/hit/bear-off classification, native history replay, opening dice deduplication and finite/distinct audible samples; run focused Vitest and observe failures.
- [x] Implement bounded synthesis and lazy gesture-unlocked playback with mute cancellation and graceful audio failure handling; implement pure event replay and pass unit tests.
- [x] Hook legal human destination placement and successful native actions; keep loads, confirmations, undo, errors and hidden workspaces silent. Add localized Settings checkbox with save/cancel semantics.
- [x] Add browser checks using actual Web Audio nodes for placement/roll/bot events and preference save/cancel/reload, then run existing Playing regressions, full frontend unit suite and production build.
- [x] Review changed paths, publish tested frontend assets/index, verify live bundle and preserve existing data. Leave source uncommitted with the ongoing play-mode work.


Observed checks: initial unit red (seven assertions failed for absent classification/synthesis), then eight green; explicit browser audio-resume rejection red (a cue was wrongly queued), then fixed by waiting for gesture unlock and requiring a running context. Final suite currently 303 frontend unit tests, 21 browser interaction cases, two actual GNUbg1.08.003 native browser cases with audio-node assertions, both colors/directions and 0/2-ply profiles. Final production build and live publication follow; no backend restart or database write is required.


Activated the tested frontend bundle on local8787. Nine audio browser cases also passed against the activated bundle; actual nodes carried nonzero audio and contexts were running. The live backend PID remains82754 and stored settings, play sessions, position/review counts compare unchanged. Only owned5194/8991 test servers and the exact disposable sounds database were removed. Final format/type/build and diff checks passed. Verification logs: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-sounds-verification/.

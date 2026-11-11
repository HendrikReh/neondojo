# Mandatory centered cube response

- [x] Cover required board-centered dialog, Escape/backdrop/focus behavior, explicit versioned Take/Drop, native failure, and Tutor/Coach pending feedback with browser regressions.
- [x] Add focused native dialog component anchored to rendered board, native modal focus containment, no cancel/close dismissal, responsive repositioning and prominent cube values. Move cube feedback and errors/hints inside; preserve existing action/ack semantics.
- [x] Convert supplied sounds/double.au to exact PCM asset; play once on a newly reached native bot offer after animation and at dialog presentation, honor mute/hidden workspace and keep reload silent. Preserve other audio behavior.
- [x] Review, run targeted sound/dialog/play tests, production build/unit/checks, visually inspect desktop/mobile and update locally served app.

Verified: 327 frontend unit tests, 4 dialog browser cases, 13 audio cases, 38 existing play/mode/board-control cases passed. Production bundle at8787 passed all6 new cube/audio cases. Read-only review found no product blockers; updated delayed-decode mute assertion for fourth recording and reran it successfully. PCM format/sample count/RMS test, build/type checks, Prettier and diff checks passed. Desktop/mobile screenshots inspected. No backend changes or test data in the user database.

# Opening dice on the board

Use the actual board dice renderer for the opening roll. Show one white and one black die, on opposite board halves, with the active theme artwork and shared rolling animation. Pending dice are subdued and labeled unrolled; native per-side results, ties, reload and same-pair first-turn logic remain unchanged. Remove the separate HTML dice cards; keep the opening controls/result explanation.

- [x] Strengthen opening UI regression to require dice inside the board SVG and none outside; observe red.
- [x] Share the built-in/archive dice renderer, add one-die-per-player opening mode and per-die orientation, route opening animation/results into it, remove external dice cards.
- [x] Verify exact theme assets, on-board placement/rolling/ties/reload, existing animations/audio and actual GNUbg opening/smoke paths; build and review diff.
- [x] Activate tested frontend, smoke-test live, preserve stored state and clean owned test resources. Leave source uncommitted.


Verified: original regression failed because no opening dice existed inside the SVG. Shared renderer now serves both built-in/archive ordinary dice and per-owner opening dice. Four new unit cases check all archive asset references, normal same-color rendering, pending labels and animation reuse; 313 units and production build pass. Four browser configurations check actual on-board bounds, opposite halves under mirroring, exact theme assets, ties and reload; 11 opening/animation and 30 appearance/input/audio regressions pass. Five real GNUbg1.08.003 cases (three seeded openings including repeated ties, plus native White/right/2-ply and Black/left/0-ply play) pass. Scoped independent code review approved with no findings. Published tested frontend assets/index; final live checks follow. Backend unchanged.


Activated local8787 and verified all four on-board opening configurations against the live bundle. Exact saved play/settings rows and position/review counts compare unchanged; backend remains PID82754. Cleaned up only owned5194/8991 services and the exact disposable boardopening test database. Format/diff checks pass; source remains uncommitted. Evidence: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-board-opening-verification/.

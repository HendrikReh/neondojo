# Resign current game and remove dice focus frames

User approved current-game resignation with single/gammon/backgammon selection and explicit confirmation. Use the existing draggable board-centered dialog, Lucide icon plus visible label below Undo, and native GNUbg scoring. Declined offers retain the exact checkpoint and remain visible in the dialog. Keep pending Coach/Tutor confirmations, revision locking and library synchronization intact. No production database writes during tests.

- [x] Reproduce dice frame: Chrome's SVG mouse focus creates outline:auto despite :focus-visible rule. Browser regression fails on actual computed outline.
- [x] Remove dice outlines for all focus states and brightness highlight; preserve consumed opacity and half-used doubles.
- [x] Add failing native integration coverage for accepted resignation levels, rejection/atomicity, version/phase validation, next game and synchronized result/SGF.
- [x] Add validated resign action with level1–3, native accept/decline detection and unchanged checkpoints on decline; regenerate OpenAPI/client schema.
- [x] Add failing browser coverage for toolbar order, modal cancel/no writes, selected award, exact one versioned request, native failure and completion.
- [x] Implement localized resignation dialog and side action, pending/busy guards, score preview, error retention and focus restoration. Offer the same action inside the mandatory Next roll dialog.
- [x] Run appropriate backend/native tests in isolated SQLx databases, frontend unit/build checks and mocked browser regressions; inspect desktop/mobile screenshots.


Verification:

- Dice regression failed with Chrome outline:auto after a mouse click before the CSS fix; passes afterward for both players/directions and built-in/archive artwork. Keyboard Enter/Space remain supported without an outline or brightness filter; full and half-used dice retain fading.
- 332 frontend unit tests pass; TypeScript/Vite production build passes.
- 28 initial mocked play regressions pass;13 focused resignation/dice checks pass after keyboard/cube/localization refinements;10 action/resignation checks pass after the localized native-decline notice. Desktop1440 and mobile390 screenshots inspected.
- Full32 native play integration tests pass in isolated SQLx databases. Parent independently reran5 resignation-related tests: all pass, including decline/atomicity, both colors, cube levels, match completion, next game, protected pending decisions and hint discard.
- Review found no material issues. Addressed the minor decline UX issue with a localized notice that does not recommend reloading or discard partial moves.
- OpenAPI/client schema regenerated. Backend binary built; activation uses graceful shutdown after checking no active analysis jobs, then verifies the live resign schema and saved-session fingerprint.


Follow-up: resulting match score preview

- [x] Add a failing mocked browser check for both player colors, an active cube of2, exact-target and above-target match completion, and switching back to a nonterminal offer.
- [x] Pass current scores, match length and both player names from the saved session into the dialog; derive the projected award locally without changing the session.
- [x] Show resulting scores in white/black order, match target and a bold conditional match-end/continue message in all6 locales. Match-ending text uses the destructive action color. Preserve points above the target.
- [x] All8 resignation browser cases pass, including both colors, exact/above-target scores, switching back to continuing play and German text. Desktop1440/mobile390 screenshots inspected; build and332 unit tests pass.

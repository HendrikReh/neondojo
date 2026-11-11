# Confirm turns by clicking dice

After the final native-legal checker step, clicking either displayed die commits the pending turn. Before moving, dice still swap their order; after a partial turn, while animations/API work run, and during opening display they cannot confirm. Keep undo/reset; undo revokes confirmation until a complete legal route is restored. Remove the dedicated Confirm Turn button and update instructions. Preserve explicit forced-pass behavior. Keyboard Enter/Space activates dice. A duplicate click must not cause duplicate engine actions. Complete means native-legal completion, including doubles and turns using only one die.

- [x] Observe failure for dice confirmation/absence of dedicated button in browser regression.
- [x] Add guarded confirm behavior to shared dice interaction, update readiness styling/labels and remove old button.
- [x] Migrate existing browser drivers to click actual dice, verify partial/undo/single-die/double/duplicate/animation cases, units/build and native full matches.
- [x] Activate tested frontend, verify live behavior and clean owned resources. Preserve saved games and leave ongoing source uncommitted.


Observed red: completed turns left dice disabled without confirm action. Shared dice now prioritize guarded confirmation over swapping; readiness is computed from complete native-compatible ordered routes and excludes API/animation work. Opening dice explicitly disable confirmation. The old HTML button is removed; ready text and tooltip labels explain the dice action in all six languages, and ready dice retain full opacity. Six new browser cases pass: either die/mouse/keyboard across colors/directions/themes; no partial submit; undo invalidates readiness; duplicate activation submits once; doubles and one-usable-die turns commit properly. All 43 play/source/animation/audio browser cases and 312 units/build checks passed. Scoped guard/race review approved. Native full matches are running with physical dice confirmation.


Completed native validation: both complete3-point matches (White/right/2-ply and Black/left/0-ply) confirmed every human checker turn via physical dice clicks; three native opening/tie, three cube/profile/reload/next-game and two normal native/audio cases also passed (10/10). SGFs retain actual configured search-depth markers. New client guards were independently reviewed with no findings.

Activated tested frontend8787; all six dice-confirm cases passed again live, covering either die/keyboard, duplicate actions, undo, doubles and single-usable-die completion. Live index equals staging. Backend/migrations unchanged; no saved sessions reset by this frontend update. Cleaned only owned5194/8991 processes and the exact disposable diceconfirm database. Build,312unit,43browser,10native,6live,format/diff checks passed. Source remains uncommitted. Evidence: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-dice-confirm-verification/.

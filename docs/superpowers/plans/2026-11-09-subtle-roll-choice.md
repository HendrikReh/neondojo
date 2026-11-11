# Subtle Cube/Roll choice

Replace the centered, blocking modal with a compact non-modal dialog in the existing sidebar (below the board in the narrow single-column layout). Keep exactly one Cube/Roll choice per human pre-roll turn, hidden during opening, API work and animations. Keep disabled-cube explanation accessible without a large paragraph. Remove backdrop, large spacing, close-to-view-board action and redundant reopening control/state: the board is always visible and accessible.

- [x] Prove current modal blocks/overlaps the board with desktop/mobile browser regression.
- [x] Render a compact non-modal choice inside the sidebar, remove modal/defer plumbing and update CSS.
- [x] Verify no board overlap, background settings/navigation remain usable, roll/cube gating and existing input/animation/audio flows; build and inspect responsive screenshots.
- [x] Publish tested frontend and check live layout. Preserve game state and clean owned test resources; leave source uncommitted.


Observed red: old choice matched :modal=true and blocked/covered the board. Current control is an inline non-modal dialog in the sidebar with a small muted heading and two buttons; no backdrop, autofocus trap, close/reopen duplicate or large explanation paragraph. Desktop1440px and narrow390px browser cases verify zero board overlap, compact bounds, no horizontal overflow, background Settings/navigation usability, legal Cube gating and exactly one Roll action. Screenshots inspected. All 312 unit tests, 12 choice/input browser cases, 20 animation/audio regressions, build/format/diff checks passed. Final typography overrides the global modal-heading styling for the compact panel.

Activated tested frontend on8787; both responsive checks passed again against the live bundle and its index matches staging. Backend/database were not changed by this UI update. Only owned5194 Vite process stopped. Source remains uncommitted. Evidence: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-subtle-choice-verification/.

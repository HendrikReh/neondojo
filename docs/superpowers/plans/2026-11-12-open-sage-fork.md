# Local Open Sage fork and 1-ply release

Goal: activate pinned fork commit 2e71bee and release 1-ply only after native and application validation.

- [x] Preserve all existing tracked/untracked changes in an isolated worktree and backup.
- [x] Build a non-editable ARM64 wheel from the exact fork commit, lock wheel hashes, install in a separate environment, run full 1-/2-ply compatibility probe.
- [x] Add failing tests for 1-ply acceptance, exact saved depth, old-install rejection and per-depth benchmark identities.
- [x] Extend the existing engine profile and shared configuration to depths 1/2; retain exact per-job/session snapshots and actual thread-budget/RECORD checks. Probe the requested depth before saving work. Keep old environments for old snapshots.
- [x] Run Python, Rust, frontend and isolated PostgreSQL/native tests, plus release build. Review only this task's diff against the preserved baseline.
- [x] Back up database/runtime, activate through revision-checked installation API and normal restart, verify user-table digests and unchanged preferences. Keep rollback files and both environments.

A new migration widens only the two Sage depth constraints to 1/2; no rows/defaults are changed. No default-depth changes, upstream commits or pushes are part of this task. The fork wheel keeps the upstream version but is distinguished by its pinned commit, wheel hash and runtime artifact fingerprint. 3/4-ply and rollouts stay unavailable.

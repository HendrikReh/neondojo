# Backend guidance (`backend/`)

These rules extend the root [AGENTS.md](../AGENTS.md) for Rust, SQLx, PostgreSQL migrations, and engine integration under `backend/`.

## Rust and Tokio

- Avoid `unwrap()` and `expect()` on untrusted input or fallible runtime resources. Propagate errors with context and map them deliberately at the API boundary (`backend/src/api.rs`).
- Take `Json`, `Query`, `Path` and `Multipart` from `crate::json_errors`, so rejected requests return the JSON `ErrorBody`; `clippy.toml` bans Axum's own. [Errors](../docs/development/api.md#errors)
- Run blocking and CPU-intensive work off Tokio's async worker threads.
- Keep `strip = "none"` in the workspace [Cargo.toml](../Cargo.toml) (`dev` and `release` profiles) for macOS 27 / Rust 1.94.1 procedural-macro compatibility.
- Keep study answer validation and equity evaluation in the shared practice module (`backend/src/practice.rs`).

## Integration tests (`backend/tests/`)

- `backend/Cargo.toml` sets `autotests = false` and links all integration tests into one binary (`integration`) via [backend/tests/integration.rs](tests/integration.rs). When adding a new test file under `backend/tests/`, register it with a `mod` line in `integration.rs`; the test `every_test_file_is_a_module` fails until you do. A large module is a directory whose `mod.rs` holds the shared helpers and the `mod` lines of its topics, like [backend/tests/play/](tests/play/mod.rs).
- Filter a single test module with:

  ```sh
  cargo test -p neondojo --test integration <module>::
  ```

  Watch for substring overlaps: `play::` also selects `open_sage_play::`, and `settings::` also selects `open_sage_settings::`. Exclude overlaps with `-- --skip open_sage_play::` (or use `cargo nextest run -E 'binary(integration) & test(/^play::/)'`).
- Because all integration test modules share one process, pass `-- --test-threads=1` when running tests that need exclusive access to a process-wide resource (such as the NeonGammon worker).
- Reuse shared HTTP helpers from [backend/tests/support/mod.rs](tests/support/mod.rs) (`use crate::support;`).

## SQLx and test isolation

- `cargo test --workspace` runs all PostgreSQL-only `#[sqlx::test]` cases by default. Always export a maintenance `DATABASE_URL` with `CREATEDB` before running it:

  ```sh
  export DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres
  ```

  Without an exported value, tests read `.env`, which points at the local user library (`.../neondojo`). Use `cargo test --workspace --lib` for pure Rust library tests without PostgreSQL.
- Mark a test `#[ignore = "requires …"]` **only** when it needs an external resource beyond PostgreSQL (GNUbg, NeonGammon, Anki, opening books, local corpora, or large synthetic datasets), and name that resource in the reason. Keep long waits out of the default suite by injecting short intervals (as `AppState::with_job_heartbeat` does).
- Pure OpenAPI or fixed-reference checks need no database; write them as `#[test]` using `api::ApiDoc::openapi()`.
- Do not run two test processes against the same PostgreSQL test server simultaneously for the same test paths — SQLx derives database names from test paths and will collide on `_sqlx_test.databases`.

## Migrations and backups

- [backend/migrations/0001_schema.sql](migrations/0001_schema.sql) is the immutable schema baseline consolidating former migrations 0001–0084; [backend/src/schema_migrations.rs](src/schema_migrations.rs) adopts older databases automatically. Never edit `0001_schema.sql` or already-applied migrations.
- Add new schema changes as sequential migrations after the latest file in `backend/migrations/`. Remember that every migration changes the backup schema fingerprint — verify backup/restore impact in [backend/src/database_backup.rs](src/database_backup.rs).

## Engine integration

- Convert GNUbg's native 0-ply numbering only at the GNUbg process boundary (`engine::gnubg_plies`); keep everything stored and exposed in NeonDojo's unified 1-ply convention.
- Keep NeonGammon's internal identifier as `bgsage` across DB schemas, settings, and APIs, while presenting it as **NeonGammon** in user-facing output.
- Enforce wheel/model integrity and real-position capability probes in [backend/src/engines/](src/engines/) (`worker.py` and `process.rs`), and never silently fall back to another engine on failure.
- Run Python tests for the sidecar and helper scripts with `npm run test:themes --prefix web` (which runs `unittest discover -s tools -p 'test_*.py'`). [Verification](../docs/development/testing.md#verification)

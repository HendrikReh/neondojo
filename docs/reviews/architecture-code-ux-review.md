# Architecture, Code, Performance & UX/UI Review: NeonDojo & NeonGammon

**Audit Date:** November 22, 2026  
**Scope:** Cross-repository read-only engineering and UX/UI audit of **NeonDojo** (`/Volumes/OWC Envoy Ultra/Development/bgtutor`) and **NeonGammon** (`/Volumes/OWC Envoy Ultra/Development/bgsage`), including their IPC, threading, and data contracts.  
**Target Platform & Invariants Respected:** Apple Silicon macOS (`arm64`, 10P+4E M4 Pro, 64 GB RAM; Modal Linux x86-64 AVX2 build exception preserved), `strip = "none"` in `Cargo.toml`, immutable `0001_schema.sql`, unified XG 1-ply convention (converted only at the GNUbg boundary), internal `bgsage` engine ID vs user-facing **NeonGammon**, fail-closed engine guarantees (no silent cross-engine fallback), dense desktop viewports (`1800×1169`, `2560×1440`, `3840×2160`), 6 UI locales (`EN`, `DE`, `ES`, `FR`, `IT`, `PL`), and a decoupled HTTP/SSE backend kept ready for a future Tauri shell.

> **Live UI Inspection Note:** Read-only live browser inspection on `http://127.0.0.1:8787` completed via the `browser` subagent (`92e170a7-f527-464b-b78a-ea032f700d19`; host-verified `visited_origins: http://127.0.0.1:8787`, `blocked_origins: none`, `latest_screenshot`: `/Users/hendrik/.gemini/antigravity/brain/12492bdd-facf-41b1-bd2f-f8a618ddf2aa/browser_evidence/92e170a7-f527-464b-b78a-ea032f700d19/final_step_26.png`). Live renders across `Playing`, `Analysis`, `Study -> Overview`, `Positions -> All analyzed decisions`, `Match database`, and `Settings -> Appearance` (`1800×1169` and `2560×1440`, non-destructive) confirmed zero horizontal overflow while visually reproducing key contrast, truncation, decimal-locale, and layout issues below. (Multi-locale `DE`/`ES`/`FR`/`IT`/`PL`, `3840×2160`, and live Coach/Flashcards interactive states remain statically audited from TSX/CSS/Playwright specs.)

---

## 1. Executive Summary

### Health Scorecard (1–5 Scale)

| Component / Surface | Performance | Modularity | Clean Architecture | Extensibility | UX / UI | Composite |
| --- | :---: | :---: | :---: | :---: | :---: | :---: |
| **NeonGammon C++17 Core** (`cpp/`) | 3.5 | 2.0 | 2.5 | 2.0 | — | **2.5 / 5** |
| **NeonGammon Python API & Training** (`python/`) | 3.0 | 2.0 | 2.5 | 2.0 | — | **2.4 / 5** |
| **NeonDojo Rust Backend** (`backend/`) | 3.0 *(DB/Jobs)* | 2.5 | 3.0 | 2.5 | — | **2.8 / 5** |
| **Cross-Repo Engine Seam** (`process.rs` ↔ `worker.py` ↔ `neongammon`) | 2.5 | 2.5 | 2.5 | 2.5 | — | **2.5 / 5** |
| **NeonDojo Frontend** (`web/src/`) | 2.5 | 2.0 | 3.0 | 2.5 | — | **2.5 / 5** |
| **NeonDojo UX/UI** (`Playing`..`Extras`, `Settings`, `Profiles`) | — | — | — | — | **3.2** *(Usability 3.0 · Visual 3.0 · Keyboard 3.5)* | **3.2 / 5** |

### What Already Works Well (Preserve!)

1. **Backgammon Correctness & Persistence Guardrails (NeonDojo Backend):** `schema_migrations.rs` pre-baseline adoption + immutable `0001_schema.sql`; `database_pool.rs` cancelled-`BEGIN` rollback protection + `SET jit = off`; `position_retention.rs` preserving saved positions across match deletion; `annotations.rs` `FOR KEY SHARE` locking; `jobs.rs` / `match_jobs.rs` leases and checkpoints; isolated `job_events` `LISTEN` pool (`max_connections(1)`) + replay-safe SSE (`api.rs:998-1032`); single-definition `player_match_sides` PR scoping; `utoipa` → `generated-api.d.ts` codegen.
2. **Engine Core Foundations (NeonGammon):** Allocation-lean generation-stamped `MoveDedup` move generator (`moves.cpp`); unified `cubeful_eval_recursive` + `NodeCapture` (`cube_eval.cpp`) and `run_trial_unified` VR contracts (`rollout.cpp`); O(1) combinatorial one-sided bearoff indexing (`bearoff.cpp`); monotonic `eval_identity()` / `BearoffDB::identity()` / `cube_state_fingerprint()` cache keying; `BatchModels` + `CHEAP_POSITIONS_PER_WORKER = 8` auto-threading + native `score_checker_position` parity; deterministic seeded `simulation.py`, `luck.py`, and MLX `2e-5` parity tests.
3. **Frontend Infrastructure & Visual System (NeonDojo Web):** Shared SSE job multiplexer (`job-watch.ts`); `AbortController` + generation-token stale-response protection (`useGameNavigation.ts`, `useJobs.ts`); optimistic revision + `localStorage` journal + rebase recovery in `configuration-store.ts`; server-side pagination across large tables; clean transparent underlined `text-navigation.css`, strict `typography.css` scale, Lucide outline iconography, 6 palette themes (`ui-appearance.css`), Check-Answer vs FSRS-Rating separation (`StudyCardExercise.tsx`), and multi-engine `PositionComparison.tsx`.

### Top Systemic Risks

1. **Batched Accelerate SME/AMX Bypassed on Production Paths:** `BearoffStrategy` omits `batch_evaluate_*` overrides, `Strategy::best_move_index_cubeful_multi` loops scalar `evaluate_probs`, `bindings.cpp` 1-ply batch/score paths run scalar loops (with `max_cube_value = 0` drift and missing `clamp_probs_to_board`), `analyzer.py` loops 1-ply `checker_play` serially in Python, and NeonDojo's `analyze_rolls` / `SageEngine::game` send 1-by-1 JSON/stdio RPCs with double SHA-256 `verify()` passes (`~33.7 MB` per call). Meanwhile, `neural_net.cpp:861-872` (`forward_from_base`, non-Accelerate / Modal AVX2) silently truncates deltas past 64 inputs.
2. **Hidden Correctness & Contract Drift Across Seam Boundaries:** `play/open_sage.rs:420,442,481` hardcodes `rating_decisions: 0` (silently zeroing rated decisions/relevance deltas in NeonGammon Play & Study); `similar_positions.rs:288-305` ignores `settings.analysis_engine` (`settings.gnubg()`) and streams the entire `positions` table into Rust; `study.rs:138` and `StudyReports.tsx:245-251` both hardcode `'GNUbg'`; `PostMoveAnalysis`, `jacoby`/`beaver` docs/defaults, and `FullStrength` 3-ply cube-vs-checker depth diverge quietly.
3. **M4 Pro Contention & PostgreSQL Scaling Cliffs:** Uncoordinated CPU pools (`CPU_BUDGET` ignores GNUbg; `MultiPlyThreadPool` uses 14 cores = 10P+4E with static striding; `ENGINE_WORK.write()` stalls Play behind benchmarks), shared `91432302` advisory lock collision (`jobs` vs `engine_matches`), `max_connections = 10` pool saturation, `O(N²)` `played_record_fingerprint` trigger amplification in `copy_children`, armed `0086`/`0089`/`0090` triggers during backup `COPY FROM STDIN`, and unindexed/full-scan queries (`similar_positions`, `duplicate_matches`, `position_browser`).
4. **Mega-File Sprawl & Coupled Boundaries:** 64% of NeonGammon C++ lives in 4 files (`bindings.cpp` 5,749 LOC, `rollout.cpp` 3,236, `neural_net.cpp` 3,044, `training.cpp` 1,437) + `analyzer.py` (1,909 LOC) + 146 uncurated `scripts/`; NeonDojo stacks 7 Rust files >1k LOC + sealed `Gnubg|OpenSage` enums across 12+ files, and a flat 427-file `web/src/` + 1.4 MB single JS bundle + 7 TSX mega-components (`PlayWorkspace.tsx` 2,497 LOC) + inverted CSS cascade order.
5. **Preventable UX Friction & Contrast Regressions:** Coach mode breaks `Enter`-to-confirm keyboard flow (`PlayCoachFeedback.tsx`), global alerts render below Playing (`App.tsx`), `play.css` uses undefined CSS vars and unreadable white-on-pastel dark-mode cube badges, Analysis hides whole-match triggers and gates 2-column Statistics behind `@media (min-width: 3000px)`, Flashcards bury content under 5 chrome rows, and Positions/Decisions/Players/Extras use inconsistent table, input, and button patterns.

---

## 2. Top 12 Highest-Leverage Improvements

| Rank | ID(s) | Improvement | Repo / Owner | Impact / Effort | Phase | Key Dependencies / Notes |
| :---: | --- | --- | --- | --- | :---: | --- |
| **1** | `NG-CPP-01`, `NG-CPP-02`, `NG-CPP-03`, `NG-PY-02`, `ND-SEAM-02` | **Restore end-to-end C++/BLAS batching & fix 64-delta truncation:** Fix `forward_from_base` `[256]`/fallback (`neural_net.cpp:861-872`), align arm64 scalar `forward()` to `sigmoid_inplace_neon`, add `BearoffStrategy::batch_evaluate_*` & batched `best_move_index_cubeful_multi`, unify C++ 1-ply candidate batching (with `max_cube_value`, `double` Janowski, `clamp_probs_to_board`), thread `force_boards`/`beaver`/`max_cube_value` through `batch_checker_play`, and batch NeonDojo `analyze_rolls` + `SageEngine::game` RPCs. | NeonGammon (`cpp`, `python`) + NeonDojo (`engines/`) | **P0** · M | A → B | Keep `SERIAL_CUBEFUL_1PLY = True` startup probe happy (1-ply candidate BLAS runs single-threaded without calling `checker_play_nply`). Do not nest `parallel_evaluate=True` inside outer `multipy_parallel_for`. |
| **2** | `ND-SEAM-01`, `ND-SEAM-04`, `NG-PY-05`, `NG-PY-01` | **Eliminate 33.7 MB per-RPC `verify()` hashing & private `worker.py` hacks:** Wrap `analyze_rolls`/game batches in `VerifiedBatch`, use stat-guarded steady-state verification on persistent `Slot::Play` workers (full SHA-256 at init/batch boundaries), release Python GIL in `bindings.cpp` single-eval/cube/load calls, deduplicate `BgBotAnalyzer` double 24-NN loads, and expose public `top_two` + `DifficultyPolicy(analyzer=..., eval_level=..., reseed)`. | NeonDojo (`worker.py`) + NeonGammon (`analyzer.py`, `difficulty.py`, `bindings.cpp`) | **P1** · S–M | A → B | Fixes 3-ply `FullStrength` checker-vs-cube depth asymmetry (`open_sage.rs:524-547`) and cuts Play worker memory. |
| **3** | `ND-BE-01`, `ND-UX-03` | **Fix silent NeonGammon correctness/attribution gaps:** Unify `rating_decisions` in `play/feedback.rs` (remove hardcoded `rating_decisions: 0` in `play/open_sage.rs:420,442,481`), route `similar_positions::score` through active `analysis_engine` instead of `settings.gnubg()`, and expose `bot_name`/`engine` on `StudyReportSummary` (`study.rs:26-45,138` + `StudyReports.tsx:245-251`). | NeonDojo (`backend`, `web`) | **P1** · S | A → B | Fixes silent 0-rated NeonGammon Play/Study PRs and misattributed `'GNUbg'` study reports immediately. |
| **4** | `ND-SEAM-03`, `NG-CPP-05`, `NG-CPP-04` | **Align M4 Pro (10P+4E) CPU budgets & thread-safe caches:** Cap `MultiPlyThreadPool` to 10 P-cores + atomic index stealing across 21 rolls, put GNUbg + NeonGammon under one Rust `CPU_BUDGET` reserving Play permits, isolate `benchmark_exclusive` from starving live Play (`ENGINE_WORK.write()`), fix `cache_salt_` raw-pointer aliasing (`eval_identity()`), `SharedPosCache` `STATE_CLAIMED` race & 288 MB eager footprint. | NeonGammon (`multipy.*`) + NeonDojo (`engines/process.rs`, `engine.rs`) | **P1** · M | A → B | Empirically verified `test_2ply_details` + `test_vr_and_dead_cube` already pass (46/46); fixing `cache_salt_` removes remaining pool-worker aliasing. |
| **5** | `ND-SEAM-05`, `ND-SEAM-08`, `ND-SEAM-09`, `ND-SEAM-07` | **Fix PostgreSQL scaling & concurrency cliffs:** Separate colliding `91432302` advisory lock (`jobs` vs `engine_matches`), convert `copy_children` 1-row loops to set-based inserts + add `matches_played_record_idx`, disable `0086`/`0089`/`0090` triggers during backup `COPY`, raise `PgPool::max_connections` (`10` → `24–32`), and add `/api/jobs/{id}/cancel`. | NeonDojo (`backend`, `migrations`) | **P1** · S–M | A | Preserves `0001_schema.sql` immutability; add sequential migration + check `backup_policy`. |
| **6** | `ND-SEAM-06` | **Eliminate full-library `positions` scan & add hot search indexes:** Add stored/generated coarse profile columns (`pip_*`, `borne_off_*`, `bar_*`, `is_contact`) + B-tree index so `similar_positions.rs` pre-filters in SQL; add `matches(coalesce(source_database_id, database_id))` & active `jobs` partial indexes; defer post-pagination joins in `position_browser.rs`. | NeonDojo (`migrations`, `similar_positions.rs`, `position_browser.rs`) | **P1** · M | B | Reconcile `recurring_mistakes/search.rs` with `DecisionSearch` `NumberedSearchPagination` (indexed early pages vs explicit cursor decision). |
| **7** | `ND-UX-06`, `ND-UX-01`, `ND-UX-05`, `ND-UX-09`, `ND-FE-03` | **Fix Play dark-mode contrast, Coach keyboard flow, alerts & header UX:** Replace undefined `--text`/`--border`/`--surface`/`--danger` tokens in `play.css`/`game-result.css` and dark-mode `.play-cube-offer strong` / `#a32929`; fix `main.tsx` CSS import order + prune dead `style.css`; move global alerts above Playing in `App.tsx`; restore Coach `Enter` confirm + Feedback/Cube shortcuts; group Match DB batch bar + `.danger`; add `Save and switch` in `ProfileSelector` + popover warnings. | NeonDojo (`web`) | **P1** · S–M | A | Fix `--ui-*` tokens **before** reordering `main.tsx` CSS imports; check `ui-themes.spec.ts` & `text-navigation.spec.ts` across all 6 palettes/locales. |
| **8** | `ND-FE-04`, `ND-FE-07`, `ND-FE-05`, `ND-FE-02` | **Cut frontend runtime & startup overhead:** Cache `Intl.NumberFormat`/`DateTimeFormat`, stop 1s `BenchmarkWorkspace` polling when idle & drive `MatchDatabase`/`AnalysisCatalog` off `job-watch.ts` SSE, fix `PlayHistory` memo & 120 Hz drag CSS vars, lazy-mount `SettingsDialog`, and code-split idle workspaces, 6 locales, and `generated-themes.json` out of the 1.4 MB main JS bundle. | NeonDojo (`web`) | **P1** · M | A → B | Keep visited workspace hooks/state alive after first open; move `validateCatalogs` out of prod startup. |
| **9** | `ND-BE-03`, `ND-FE-09`, `ND-BE-05` | **Unify HTTP error/validation contracts across Rust & TS:** Add shared `ApiJson`/`ApiQuery` extractors in `api.rs` (stop plain-text 400/422 leaks), fold `code`/`issues` onto `ErrorBody`, delete `ConfigurationApiError`/`SettingsApiError` + `settings-api.ts` legacy wraps so `configuration-api.ts` uses `api.ts` + `generated-api.d.ts`, fix `configuration/store.rs:258` `play.engine` validation gap, and share `normalized_loss` + `ACTIVE_COMPETITION`. | NeonDojo (`backend`, `web`) | **P1** · M | A → B | Run `npm run api:types --prefix web` and `npm run typecheck:e2e --prefix web`. |
| **10** | `ND-FE-01`, `ND-FE-06`, `ND-UX-02`, `ND-UX-03`, `ND-UX-04` | **Decompose frontend mega-components & remove Analysis portals:** Feature-slice flat 427-file `web/src/` (`src/{app,shared,workspaces,settings}/`); replace `AnalysisWorkspace`'s 4 `createPortal` targets + `MutationObserver` with `useMatchAnalysisController` (exposing `Analyze match …` directly); drop `3000px` `@media` gate on 2-col Statistics; split `PlayWorkspace.tsx` (2,497 LOC) & `LearningPanel.tsx` (1,633 LOC, decluttering 5 chrome layers & `AddCardsPanel`); unify `Positions`/`Decisions`/`Similar` table UX. | NeonDojo (`web`) | **P1** · L | B → C | Preserve `views` order, `workspace-tab-*` IDs, `NumberedSearchPagination`, and Check-vs-Rate Study separation. |
| **11** | `ND-BE-01`, `ND-BE-02`, `ND-BE-04`, `ND-BE-09` | **Modularize Rust backend & add `EngineProvider` + `backgammon/` domain:** Split 7 >1k-LOC modules into `routes`/`service`/`repo`; extract `Board`/`GnubgPositionId`/`GnubgMatchId`/legal-move rules out of `import::sgf` & `ids.rs`; define `EngineProvider` capability trait in `engines/` (ending 12-file scatter); remove runtime `unwrap()` panics and extract `neondojo::runtime::start` for Tauri. | NeonDojo (`backend`) | **P1–P2** · L | B → C | Preserves HTTP paths, OpenAPI contract, and `"bgsage"` DB IDs. |
| **12** | `NG-CPP-09`, `NG-CPP-10`, `NG-PY-07`, `NG-PY-08`, `NG-PY-03` | **Close NeonGammon API parity, wheel bloat & pure-Python hygiene:** Prune 6 unused `sl_s9_*.weights.best` (2.08 MB) + `_tdinit` sdist filter (`pyproject.toml`/`CMakeLists.txt`), make `__init__.py` lazy (PEP 562) so `text_export` works without `bgbot_cpp`, fix `post_move_analytics` `cubeful`/cube-life parity + `jacoby`/`beaver` docs, extract `bgbot::AnalyzerSession` for `mobile.cpp` parity, and move to manifest-driven `RoutedStrategy` for Stage 12+. | NeonGammon (`cpp`, `python`, `mobile`) | **P1–P2** · M–L | A → C | Run `test_encoding_golden.py`, `test_2ply_details.py`, `mobile/test_parity.py`, and wheel smoke tests. |

---

## 3. Cross-Repo Seam Analysis (`NeonDojo ↔ NeonGammon`)

NeonDojo integrates NeonGammon across the boundary:

`NeonDojo Rust (engine.rs / engines/{mod,process,game}.rs / play/open_sage.rs)` → `JSON/stdio` → `backend/src/engines/worker.py` → `neongammon Python (analyzer.py / batch.py / difficulty.py)` → `pybind11 (bindings.cpp)` → `bgbot C++17 (neural_net.cpp / multipy.cpp / cube_eval.cpp / bearoff.cpp / rollout.cpp)`.

Cross-examination between `nd-perf-seam`, `ng-cpp-core`, and `ng-python-api` established five concrete architectural findings at this seam:

### 3.1 Batching & 1-Ply Parity Chain (Plus Startup Probe Lockstep)
- **Current bottleneck:**
  1. **IPC level:** `AnalysisEngine::analyze_rolls` (`engines/mod.rs:699-710`) sends 21 sequential JSON/stdio RPCs; `SageEngine::game` (`engines/game.rs:62-153`) sends 1 RPC per cube and 1 per checker decision (`cube_with_batch` / `checker_boards_with_batch`).
  2. **Python level:** `BgBotAnalyzer.checker_play` at 1-ply (`analyzer.py:388-396`) loops candidate boards serially in Python (`~20–250` pybind11 crossings + per-candidate Janowski helpers). Meanwhile `neongammon.batch.batch_checker_play` (`batch.py:480-584`) omits `force_boards`, `beaver`, and `max_cube_value`, even though C++ `checker_position_from_dict` (`bindings.cpp:526-545`) already parses `force_boards` and `max_cube_value`!
  3. **C++ level:** At 1-ply, `bindings.cpp:4362-4709` loops scalar `evaluate_probs`, hardcodes `max_cube_value = 0` (`4434`), uses `float cf_eq` vs `double`, omits `clamp_probs_to_board`, and spawns ephemeral `std::thread`s; `score_checker_position:423` also scores 1-ply candidates serially without `clamp_probs_to_board`. At 2-ply+, `MultiPlyStrategy::set_bearoff_db` (`multipy.cpp:296-306`) wraps `base_` and `filter_strat_` in `BearoffStrategy`, which omits `batch_evaluate_*` overrides (`bearoff.h:164-209`) — causing leaf and filter evaluations to fall back from Accelerate `cblas_sgemm` to scalar `forward()` loops (`strategy.cpp:153-219`).
- **Four critical cross-repo guardrails agreed in Phase 2:**
  1. **Do NOT violate `SERIAL_CUBEFUL_1PLY` probe:** `probe_open_sage.py:55-94` + `worker.py:85` enforce `neongammon.analyzer.SERIAL_CUBEFUL_1PLY is True` and fail startup if 1-ply spawns `ThreadPoolExecutor` or `checker_play_nply`. Routing 1-ply `BgBotAnalyzer.checker_play` through a single-thread C++ candidate BLAS call (`batch_evaluate_candidates_equity_probs` -> one `cblas_sgemm`, zero `MultiPlyThreadPool` threads) eliminates Python loop overhead while honoring the probe intact.
  2. **Align scalar vs Accelerate sigmoid (`~0.003` shift):** Scalar `forward()` on arm64 uses `fast_sigmoid` LUT, whereas Accelerate `forward_group` uses `sigmoid_inplace_neon` (Cephes `exp`, per `CLAUDE.md:64`). Moving 1-ply scoring to batched BLAS shifts equities by `~0.003` unless scalar arm64 `forward()`/`forward_batch()` also adopts `sigmoid_inplace_neon` (`[NG-CPP-03]`) — doing both unifies single, batch, and 2-ply leaf math.
  3. **Never nest `parallel_evaluate=True` inside `batch_checker_play_nply`:** `multipy.cpp:363-367` deadlocks under nested `multipy_parallel_for`. Keep `BatchModels.multipy(parallel_evaluate=False)` for position-parallel batches (`analyze_rolls`, `SageEngine::game`), and for small batches (`len(positions) < n_threads` at 3-ply/4-ply) allocate `candidate_threads = max(1, worker_budget // n)` to `score_checker_position` (`bindings.cpp:393-401`).
  4. **Fix C++ 1-ply `batch_checker_play` contract parity:** Route both `batch_checker_play(..., n_plies == 1)` and single-position 1-ply through one shared `score_checker_1ply` C++ helper respecting `max_cube_value`, `double` Janowski, and `clamp_probs_to_board`.

### 3.2 Integrity Hashing Cost (`verify()`)
- Whenever `not self.batch_active`, `worker.py:93-112, 181-182, 291-292` calls `self.verify()` **before and after every RPC**, re-reading `RECORD` and SHA-256 hashing all `.py` files, 42 `_assets/*` files, and `bgbot_cpp*.so` (~16.85 MB × 2 = **~33.7 MB per RPC**; **42 full hashes** per `analyze_rolls` call).
- **Safe two-step fix:** (a) Wrap `analyze_rolls` and chunked `SageEngine::game` batches inside `VerifiedBatch` (`begin_batch`/`end_batch` hash once per batch); (b) on persistent `Slot::Play` / interactive RPCs, verify full SHA-256 on worker startup / profile swap / batch boundaries and use `(st_dev, st_ino, st_size, st_mtime_ns)` stat validation across the locked file inventory on steady-state turns (re-hashing immediately if any stat field changes).

### 3.3 M4 Pro (10P+4E) Thread Coordination & `ENGINE_WORK` Starvation
- Rust `CPU_BUDGET` (`engines/process.rs:20-35`) uses `14` logical cores and **only gates `bgsage`** — ignoring up to 8 concurrent GNUbg subprocesses (`engine.rs:384-406`, `jobs.rs:69`). Meanwhile, `MultiPlyThreadPool` spawns 14 persistent C++ threads per worker (`GENERAL` + `PLAY` = 28+ threads + `top_two_pool` + Accelerate BLAS) and uses static `item += workers` striding across 21 unequal opponent dice rolls. Worse, `ENGINE_WORK.write().await` (`engine.rs:10, 36-39`) locks all engines during multi-minute benchmarks, freezing interactive Play turns.
- **Target policy:** Query Apple Silicon P-core count via `sysctlbyname("hw.perflevel0.physicalcpu")` (`10` on M4 Pro) as C++ default; switch `MultiPlyThreadPool::parallel_for` to atomic index stealing; move Rust `CPU_BUDGET` into a shared coordinator gating **both** GNUbg (`set threads`) and NeonGammon (`GENERAL` + `PLAY`, reserving 1–2 permits and keeping `PLAY` `parallel_threads <= 4`); isolate `benchmark_exclusive` from live Play (or fail/reject interactive benchmark overlap without hanging Play).

### 3.4 `PosCache` / `SharedPosCache` Lifetime & `test_vr_and_dead_cube.py`
- Empirical check run read-only during this audit (`tests/test_2ply_details.py` + `tests/test_vr_and_dead_cube.py` in one process) **passed 100%** (all 46 / 41 cases): commit `893740b` (`eval_identity()`, `BearoffDB::identity()`, `cube_fp_one` in `g_cubeful_eval_cache`) fixed the historical leak documented at `test_vr_and_dead_cube.py:31-45`.
- **Two latent hazards remain:** (a) `MultiPlyStrategy::cache_salt_` (`multipy.cpp:254, 275`) hashes raw `base_.get()` pointer addresses instead of `base_->eval_identity()`, risking heap-reuse aliasing across `MultiPlyThreadPool` worker threads (`clear_cache()` only clears the calling thread); (b) `SharedPosCache` allocates `~288 MB` upfront even for 72-trial `1T`, has contiguous atomic stat counters causing false sharing, and has a `STATE_CLAIMED` probe race (`multipy.h:88-125`).

### 3.5 Replacing `worker.py`'s Private Hooks & Keep Process Isolation (ROI Call)
- `worker.py:80, 122-163, 197-214` depends on private internals (`_analyzer._inner._strategy_1ply`, `_FLIP_OWNER`, `FILTER_MAX_MOVES`, `_contact_cube_life`, `_rng`), duplicates 24-NN weights per cached `DifficultyPolicy`, and mismatches 3-ply `FullStrength` (checker at 3-ply via `self.analyzer`, cube at 2-ply via `DifficultyPolicy`).
- **Decision:** Export public `BgBotAnalyzer.top_two_promote(...)` + `resolve_owner`/`flip_owner`/`contact_cube_life` and `DifficultyPolicy(analyzer=..., eval_level=..., reseed)` — **while deliberately keeping the `worker.py` JSON/stdio process boundary** rather than rewriting to direct Rust↔C++ FFI right now. Chunked `batch_checker_play` + `VerifiedBatch` captures almost all IPC/hash latency savings while preserving crash isolation, `kill_on_drop` reaping, and wheel integrity.

---

## 4. Subsystem Deep Dives

### 4.1 NeonGammon C++17 Core (`cpp/`, `CMakeLists.txt`, `mobile/`)

- **ID & Title:** `[NG-CPP-01] BearoffStrategy omits batch_evaluate_* overrides and Strategy::best_move_index_cubeful_multi runs scalar loops on production N-ply and cubeful rollouts`
- **Severity / Effort / Pillar:** `P0` · `M` · `Performance`
- **Evidence:** `cpp/include/bgbot/bearoff.h:164-209` overrides `evaluate`, `evaluate_probs`, `best_move_index`, `best_move_index_cubeful`, and `best_move_index_cubeful_multi`, omitting `evaluate_candidates_equity`, `batch_evaluate_candidates_equity`, `batch_evaluate_candidates_equity_probs`, and `batch_evaluate_candidates_best_prob`. `MultiPlyStrategy::set_bearoff_db` (`cpp/src/multipy.cpp:296-306`, also active in `batch.py:170-177`) wraps `base_` and `filter_strat_` in `BearoffStrategy`; 1-ply leaf/filter dispatches (`multipy.cpp:654, 668, 803`) fall back to `Strategy`'s scalar `evaluate_probs()` loops (`cpp/src/strategy.cpp:153-219`). Simultaneously, `Strategy::best_move_index_cubeful_multi` (`strategy.cpp:85-123`) loops scalar `evaluate_probs`, and `BearoffStrategy::best_move_index_cubeful_multi` (`bearoff.cpp:887-906`) falls back to scalar `base_->best_move_index_cubeful_multi` when `!all_bearoff`, hitting every trial move in `RolloutStrategy` (`rollout.cpp:493-506, 815-823, 1686-1723`) plus `rollout.cpp:3164-3172` and a duplicate `strat.evaluate_probs(candidates[idx], board)` at `rollout.cpp:503`.
- **Impact:** Production N-ply (`BearoffDB` attached) and cubeful rollouts lose Accelerate `cblas_sgemm` batching, dropping to 15–30× per-roll scalar `forward()` calls plus duplicate winner evals and heap allocations.
- **Recommendation:** Implement `batch_evaluate_candidates_*` on `BearoffStrategy` using the terminal / DB / non-bearoff split pattern from `cube_eval.cpp:298-318`; rewrite `Strategy::best_move_index_cubeful_multi` and `rollout.cpp:3164-3172` to call `batch_evaluate_candidates_equity_probs` (using `thread_local` scratch); support mixed bearoff/non-bearoff batches in `BearoffStrategy::best_move_index_cubeful_multi` and return chosen `probs` (`out_best_probs`) to delete the duplicate eval at `rollout.cpp:503`.
- **Verification & Risk Guardrail:** Full `pytest` suite, `test_cubeful.py`, VR-consistency tests (`rollout.cpp:487-492`), ThreadSanitizer rollout stress, `benchmark_3t` / `benchmark_4ply`.

---

- **ID & Title:** `[NG-CPP-02] Scalar evaluation, 1-ply max_cube_value=0 drift, missing clamp_probs_to_board, and ephemeral std::thread pools in bindings.cpp batch APIs`
- **Severity / Effort / Pillar:** `P0` · `M` · `Performance`
- **Evidence:** `cpp/pybind/bindings.cpp:3658, 3796, 3927` (`batch_evaluate_positions`), `4063, 4173, 4280` (`batch_evaluate_post_move`), `4362-4709` (`batch_checker_play` 1-ply), and `423` (`score_checker_position` 1-ply scoring) invoke scalar `evaluate_probs(...)` and omit `clamp_probs_to_board` (except `batch_checker_play` only on some paths). Moreover, 1-ply `batch_checker_play` hardcodes `max_cube_value = 0` (`4434`), runs a dead filter (`4458-4467`), and uses `float cf_eq` (`4443`) vs `double` in `score_checker_position:426`. Meanwhile `bindings.cpp:582-591, 3681-3696, 3814-3829, 3945-3960, 4079-4094, 4188-4203, 4295-4310, 4482-4497, 4659-4674` spawn/join ephemeral `std::vector<std::thread>` per call.
- **Impact:** Bulk batch calls miss SME/AMX matrix batching, pay OS thread lifecycle + cold `thread_local` allocation overhead, diverge on dead-cube 1-ply math (`max_cube_value = 0`), and risk unclamped terminal probabilities.
- **Recommendation:** Consolidate position/post-move overloads onto shared templates dispatching NN-grouped `eval_each_impl`-style chunks over `multipy_parallel_for` with `clamp_probs_to_board`. Route both 1-ply `batch_checker_play` and `score_checker_position` through a single batched `score_checker_1ply` helper (`batch_evaluate_candidates_equity_probs`, parsed `max_cube_value`, `double` Janowski, `clamp_probs_to_board`), and move `batch_checker_play_nply` onto `multipy_parallel_for` (respecting `SERIAL_CUBEFUL_1PLY`).
- **Verification & Risk Guardrail:** `test_batch.py`, `test_batch_checker_play_parity.py`, `test_cubeful.py`, and NeonDojo's `probe_open_sage.py` (`validate_thread_budget`).

---

- **ID & Title:** `[NG-CPP-03] NeuralNetwork::forward_from_base 64-delta truncation bug, scalar arm64 forward()/forward_batch(), sigmoid mismatch, and tiny-m SGEMM overhead`
- **Severity / Effort / Pillar:** `P0` · `M` · `Performance | Architecture`
- **Evidence:** `cpp/src/neural_net.cpp:861-872` (`forward_from_base`, used by the `#else` / Modal x86-64 AVX2 / `-DBGBOT_USE_ACCELERATE=OFF` `forward_group` path at `1065-1079`) hardcodes `int delta_indices[64]; float delta_vals[64]; if (d != 0.0f && n_deltas < 64)` — silently dropping any changed input features past index count 64 out of 196/244. On Apple Silicon (`arm64`), `forward` (`388-460`) and `forward_batch` (`585-655`) have `#if defined(BGBOT_USE_AVX2)` only, executing scalar hidden sigmoids and output dot products; Accelerate `forward_group` (`1041, 1058`) mixes `sigmoid_inplace_neon` (Cephes) on hidden with `fast_sigmoid` LUT on output vs LUT-only in `forward`. `cube_eval.cpp:273-289` (`eval_groups_pair`) runs an `O(n²)` bucket scan and sends `m = 1..2` groups through `cblas_sgemm`.
- **Impact:** Silent mis-evaluation on Modal cloud rollouts / non-Accelerate builds whenever >64 inputs change; degraded scalar `forward()` throughput on Apple Silicon; BLAS overhead on tiny groups; `~0.003` equity drift when switching 1-ply paths from scalar `forward` to Accelerate BLAS.
- **Recommendation:** Immediately fix `forward_from_base` (use `[256]` or fall back to full `forward(inputs)` when `n_deltas >= 64`). Vectorize arm64 `forward()`/`forward_batch()` with `sigmoid_inplace_neon` + NEON output FMA (eliminating LUT vs Cephes drift), add a `count <= 2` fast path before `cblas_sgemm` in `forward_group`, and replace `eval_groups_pair`'s `O(n²)` scan with fixed `std::array` bucket counts writing directly into `encoded.data()`.
- **Verification & Risk Guardrail:** New unit test comparing `forward_from_base` vs `forward` on >64 changed inputs (`-DBGBOT_USE_ACCELERATE=OFF` and Linux AVX2); `test_encoding_golden.py`, `test_2ply_details.py`, `profile_nn_inference`.

---

- **ID & Title:** `[NG-CPP-04] Cache memory bloat (~288 MB), false sharing, STATE_CLAIMED race in SharedPosCache, and raw-pointer cache_salt_ aliasing`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance`
- **Evidence:** `cpp/include/bgbot/multipy.h:23-51` (`SharedPosCache`) allocates `CAPACITY = 2 * 1024 * 1024` (~288 MB upfront per multithreaded `RolloutStrategy`, even for 72-trial `1T` at `rollout.cpp:309-311`), with contiguous atomic counters `hits`/`misses`/`inserts` (`47-49`, and `PosCache::global_hits`/`global_misses` at `402-403`). `lookup_or_reserve` (`88-125`) fails `(s == STATE_COMPUTING || s == STATE_READY)` at line `103` when racing on `STATE_CLAIMED`, letting two threads compute the same position. `MultiPlyStrategy::cache_salt_` (`multipy.cpp:254, 275`) hashes raw pointer `base_.get()` instead of `base_->eval_identity()`. `g_cubeful_eval_cache` (`cube_eval.cpp:174-226`) allocates ~7 MB eagerly per thread and `MultiPlyStrategy::clear_cache()` (`multipy.cpp:317`) sweeps 32,768 entries per call (`cube_eval.cpp:182-184`) despite identity-keyed entries.
- **Impact:** Heavy RSS bloat on lightweight truncated rollouts, cross-core cache-line bouncing, redundant speculative evaluations under races, latent heap-reuse aliasing in worker threads' `thread_local PosCache`, and O(32K) clear tax.
- **Recommendation:** Pad atomic stats (`alignas(64)`) or accumulate locally per worker; size `SharedPosCache` dynamically by `n_trials`/`truncation_depth` and allocate lazily; spin/recheck `STATE_CLAIMED` before advancing probe slot; replace `cache_salt_` with `base_->eval_identity()`; lazily allocate `CubefulEvalCache::entries` and reserve full `g_cubeful_eval_cache` sweeps for an explicit test-reset hook.
- **Verification & Risk Guardrail:** ThreadSanitizer stress on `SharedPosCache` + `PosCache`; `test_2ply_details.py` + `test_vr_and_dead_cube.py`; `benchmark_3t`.

---

- **ID & Title:** `[NG-CPP-05] MultiPlyThreadPool static striding on 10P+4E M4 Pro cores and heap churn in cubeful candidate selection`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance`
- **Evidence:** `cpp/src/multipy.cpp:37-192` spawns `hardware_concurrency()` (`14` on M4 Pro 10P+4E), pushes `std::function` items behind `queue_mutex_`, allocates a per-call `JobState` (`mutex` + `condition_variable`), and strides statically (`item += workers` at line `86`) across 21 unequal opponent dice rolls. `MultiPlyStrategy::best_move_index_cubeful_multi` (`950-1079`) allocates 7 `std::vector`s per call (including nested `vector<vector<float>> cf_equities`); `rollout.cpp:1139` leaves an unused `MoveFilter cube_filter`.
- **Impact:** Tail latency from asymmetric 21-roll stragglers and E-core scheduling on M4 Pro; inner-loop allocator contention during parallel rollouts.
- **Recommendation:** Cap default workers to P-cores via `sysctlbyname("hw.perflevel0.physicalcpu")` (`10` on M4 Pro), switch `parallel_for` to atomic index stealing (`std::atomic<int>`), move `best_move_index_cubeful_multi` locals to `thread_local` flat buffers, and delete unused `cube_filter`.
- **Verification & Risk Guardrail:** Before/after wall-clock timings on `bgbot_profile_4ply`, `benchmark_4ply`, and `benchmark_3t` (10 vs 14 threads).

---

- **ID & Title:** `[NG-CPP-06] Redundant board scans and duplicate *_fast helpers across BackgameAwarePairStrategy and encoding.cpp`
- **Severity / Effort / Pillar:** `P2` · `M` · `Performance | Modularity`
- **Evidence:** `BackgameAwarePairStrategy::select_nn_idx` (`neural_net.cpp:2604-2643`) computes `classify_game_plan`, `flip` + `classify_game_plan`, `snake_category`, `containment_category`, then calls `massive_category` (`2481-2486`) which re-runs `containment_category || snake_category`, `flip`, and `pip_counts`, then `backgame_category_given_plans` / `backgame_phase`. `encoding.cpp` duplicates 7 helpers (`679-1035` vs `*_fast` at `1346-1465`) and calls multiple separate 24-point scans in `compute_extended_contact_inputs` (`1498-1588`).
- **Impact:** Unnecessary per-candidate CPU inside `evaluate_board_group`/`eval_groups_pair` and feature-drift risk.
- **Recommendation:** Pass already-computed snake/containment/pip facts via a single-pass `BoardSummary` shared across classification/routing, and delegate `encoding.cpp`'s 7 standalone wrappers to `*_fast`.
- **Verification & Risk Guardrail:** `tests/test_encoding_golden.py` (1,000-position bit-exact check) + `profile_0ply`.

---

- **ID & Title:** `[NG-CPP-07] Monolithic C++ files, massive overload duplication, sticky stream formatting, and dead code`
- **Severity / Effort / Pillar:** `P1` · `L` · `Modularity`
- **Evidence:** Four monoliths hold 64% of C++ LOC: `bindings.cpp` (5,749 LOC: embeds `score_checker_position`, repeats 10 batch overloads across `3603-4847`, duplicates `mwc_se_to_eq_se` at `170` vs `rollout.cpp:2838`), `rollout.cpp` (3,236 LOC: duplicates ~250 LOC between `cubeful_cube_decision` and `cubeful_cube_decision_batched`, undefined `RolloutStrategy_common_init` at `281`), `neural_net.cpp` (3,044 LOC: duplicates ~600 LOC across `GamePlan*` `batch_*`), and `training.cpp` (1,437 LOC: duplicates 6 TD train/bench loops and leaves `std::cout << std::fixed` un-restored). Dead code exists in `cube.cpp:24-58` (`g_leaf_count`) and `bearoff.cpp:58-118, 503-511` + duplicate DB scan in `generate_gammon_distributions` (`563-676`).
- **Impact:** Multi-place edit bugs (e.g., cube collapse/SE math) and slow incremental rebuilds.
- **Recommendation:** After landing Phase A/B perf fixes and `AnalyzerSession`, split `bindings.cpp` into focused translation units (`bindings_{core,eval,rollout,training}.cpp`), split `rollout.cpp` (`vr`, `trial`, parameterized `cube`), unify `cube::collapse_decision` & `mwc_se_to_eq_se`, share a policy template for `GamePlan*Strategy::batch_*`, restore `std::defaultfloat`, and delete verified dead code.
- **Verification & Risk Guardrail:** CMake/pip build, full `pytest`, `test_2ply_details.py`, and bearoff generator checks.

---

- **ID & Title:** `[NG-CPP-08] Dual N-ply cube traversal when decision_only=false and dynamic_cast strategy coupling in cube_eval.cpp`
- **Severity / Effort / Pillar:** `P2` · `M` · `Architecture | Performance`
- **Evidence:** `bindings.cpp:5690-5708` (`cube_decision_nply_unified`) and `mobile.cpp:133-138`: when `decision_only=false`, C++ runs `cube_decision_nply`, then constructs a separate `MultiPlyStrategy` (`5704`) and re-traverses cubelessly solely for `probs` (while `analyzer.py:545` calls `clear_cache()` on a different instance). `cube_eval.cpp:298-370` uses `dynamic_cast<const BearoffStrategy*>`, `BackgameAwarePairStrategy*`, and `GamePlanPairStrategy*`.
- **Impact:** Up to ~2× cube-call latency when pre-roll probabilities are requested; `cube_eval` couples to concrete NN layout classes.
- **Recommendation:** Output root pre-roll `probs` directly from `cubeful_eval_recursive`'s ND 21-roll `NodeCapture` when depth/filter match, virtualize `eval_each_impl` / `pinned_post_probs` on `Strategy`, and clean up dead `clear_cache()` at `analyzer.py:545`.
- **Verification & Risk Guardrail:** `test_2ply_details.py` and `test_cubeful.py` (`probs` & `cubeless_equity` parity).

---

- **ID & Title:** `[NG-CPP-09] mobile/ duplicates C++ analyzer logic without 1-ply TINY prefilter or money contact_x parity`
- **Severity / Effort / Pillar:** `P1` · `M` · `Architecture | Extensibility`
- **Evidence:** Because `score_checker_position` (`bindings.cpp:350-507`) lives inside pybind bindings, `mobile.cpp:60-154` (`mobile.h:13-30`) reimplements strategy dispatch (`68-72`), unbatched scalar 1-ply scoring without `clamp_probs_to_board` (`101-105`), 2-ply scoring over **every** legal move with no 1-ply TINY prefilter (`99-113`), and 2-ply cube decisions (`132-138`) rebuilding `PubEval` per call and leaving `contact_x = -1.0f` (`89-91`) instead of calibrated money contact cube life (`0.7375`/`0.7625`).
- **Impact:** Mobile C ABI drifts from desktop `BgBotAnalyzer` on 2-ply move pruning and money contact cube life, wasting mobile CPU budget and leaving pure-C++/FFI callers without a unified analyzer entry point.
- **Recommendation:** Extract `score_checker_position` and strategy initialization into a pure-C++ `bgbot::AnalyzerSession` (`cpp/src/analyzer_core.cpp`) supporting prefilter, batched 1-ply, and calibrated `contact_x`, with `bindings.cpp` and `mobile.cpp` as thin adapters.
- **Verification & Risk Guardrail:** Rebuild `mobile/CMakeLists.txt` (`libneongammon_mobile`) and run `mobile/test_parity.py`.

---

- **ID & Title:** `[NG-CPP-10] Rigid Stage 11 routing, 5-point model wiring, and untagged .weights format block Stage 12+ topologies`
- **Severity / Effort / Pillar:** `P2` · `L` · `Extensibility`
- **Evidence:** `BackgameAwarePairStrategy` (`neural_net.h:591-675`, `neural_net.cpp:2535-2643`) multiplexes 19–24 weight vectors via `(weight_paths.size(), phase_containment)` constructor overloads and magic indices `0..23`, duplicated in `CMakeLists.txt:37-67` (`sl_s9_*` / `sl_s11_*` / `sl_s5_purerace` globs), `bindings.cpp:250-265`, and `mobile.cpp:68-72`. `.weights` headers encode only `[n_hidden][n_inputs][n_outputs]` without encoder version or activation/topology tags.
- **Impact:** Adding Stage 12+ models, V2 encodings, or non-1-hidden-layer topologies requires synchronized edits across 5+ C++/CMake/Python files.
- **Recommendation:** Replace vector-length constructors with a manifest-driven `RoutedStrategy` (`ModelManifest` mapping named slots, `BoardSummary` rules, root pinning, and encoder version) and versioned weight metadata / `NetEvaluator` seam when Stage 12 introduces new formats.
- **Verification & Risk Guardrail:** Wheel build smoke test + cold-start `BgBotAnalyzer()` + `test_encoding_golden.py`.

---

### 4.2 NeonGammon Python API, Training & Hygiene (`python/`, `pyproject.toml`, `scripts/`, `tests/`)

- **ID & Title:** `[NG-PY-01] Missing GIL release on single-eval, unified cube, and strategy-creation pybind11 paths, plus small-batch N-ply behavior`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance`
- **Evidence:** `bindings.cpp:1365-1407, 4942-4954, 5208-5220` (`Strategy::evaluate_board` + overrides), `5332-5447` (`create_strategy`, `create_multipy*`, `create_rollout`), and `5598-5637, 3172-3250, 2984-3027, 1648-1664, 5162-5183` (`evaluate_cube_decision_unified`, `evaluate_cube_decision*`, `classify_game_plans_batch*`) all run without `py::gil_scoped_release`. Separately, `batch.py:170-174` passes `parallel_evaluate=False, parallel_threads=0` to `BatchModels.multipy()`.
- **Impact:** Multi-threaded Python callers serialize on the GIL during 1-ply eval, cube decisions, and 24-NN disk loading; small 1–4 position N-ply batches underutilize cores unless outer concurrency saturates them.
- **Recommendation:** Acquire `py::gil_scoped_release` after extracting C++ inputs in `bindings.cpp`. Keep `BatchModels.multipy(parallel_evaluate=False)` inside position-parallel `batch_checker_play_nply` (avoiding `multipy.cpp:363-367` nested deadlock), and allocate `candidate_threads = max(1, worker_budget // n)` inside `score_checker_position` when `n < worker_budget`.
- **Verification & Risk Guardrail:** Concurrent `ThreadPoolExecutor` scaling test + `test_batch_models.py` + `test_analyzer_thread_limits.py`.

---

- **ID & Title:** `[NG-PY-02] Monolithic 1,909-LOC analyzer.py loads 24 NNs twice, double-wraps BearoffStrategy, retains dead N-ply code, and loops 1-ply in Python`
- **Severity / Effort / Pillar:** `P1` · `L` · `Modularity | Performance`
- **Evidence:** `python/neongammon/analyzer.py` (1,909 LOC = 22% of the package) builds `_strategy_1ply` in `_CubelessBase.__init__`, overwrites `inner._strategy_1ply = BearoffStrategy(...)` without saving `_raw_base` (`1467-1474`), then re-reads all 24 weights from disk in `_cube_strategy` (`207-214`), and double-wraps `BearoffStrategy(BearoffStrategy(...))` at `522, 946, 1675` (`bindings.cpp:5467-5614`). Lines `1133-1190` keep dead `_convert_move_legacy` / `_pool_map` / `_nply_eval` code unused by `_checker_play_native`, while `_OnePlyAnalyzer.checker_play_analytics` (`388-396`) loops candidate boards serially in Python (`~20–250` pybind crossings) and `_MultiPlyAnalyzer` (`424-446`) hardcodes `FILTER_MAX_MOVES=5, FILTER_THRESHOLD=0.08`.
- **Impact:** 2× memory/weight load per analyzer using cube calls, double virtual/bearoff indirection, slow Python 1-ply `checker_play`, and high maintenance cost.
- **Recommendation:** Share `_raw_base` between 1-ply and `_cube_strategy` (eliminating duplicate disk load & double `BearoffStrategy` wrap); delete dead legacy N-ply fallback; route single-position 1-ply `checker_play` to C++ batched candidate BLAS (keeping `SERIAL_CUBEFUL_1PLY = True` for `probe_open_sage.py`); split `analyzer.py` into `strategies.py`, `cubeful.py`, `rollout.py`, and `facade.py`.
- **Verification & Risk Guardrail:** `test_analyzer_model_reuse.py`, `test_batch_checker_play_parity.py`, `test_checker_play_shared_routine.py`, `test_bearoff_cache_identity.py`.

---

- **ID & Title:** `[NG-PY-03] post_move_analytics diverges from batch_post_move_evaluate and docs; batch_checker_play omits force_boards/beaver/max_cube_value`
- **Severity / Effort / Pillar:** `P1` · `S` · `Architecture`
- **Evidence:** `analyzer.py:1637-1733` unwraps `_inner`, ignoring `self._cubeful` (contradicting `types.py:201`); at 2-ply+ passes `1 if one_ply else None` to `_janowski_x` (`0.68` vs `batch.py:69` `ROOT_JANOWSKI_CUBE_LIFE_PLY = 1` & `docs/api.md:220`); calls `clear_cache()` *before* `cubeful_probs_nply` (`1655` vs `1668`); and omits `beaver`/`max_cube_value`/`contact_x`. Meanwhile, `batch.py:364-365, 480-584` drops `cube_value` on money `batch_post_move_evaluate` inputs and omits `beaver`, `max_cube_value`, and `force_boards` on `batch_checker_play` (blocking NeonDojo batching despite `bindings.cpp:526-545` support).
- **Impact:** Silent equity divergence between single-position and batch `post_move` at 2-ply+; wrong `cubeful_equity` when `cubeful=False`; NeonDojo unable to batch `analyze_rolls` / `SageEngine::game` cleanly.
- **Recommendation:** Honor `self._cubeful`, unify `ROOT_JANOWSKI_CUBE_LIFE_PLY = 1` and `contact_x`/`beaver`/`max_cube_value` across `post_move_analytics` & `batch_post_move_evaluate`, preserve money `cube_value`, and expose `force_boards`, `beaver`, and `max_cube_value` in `batch_checker_play`.
- **Verification & Risk Guardrail:** Cross-mode parity tests (`post_move` vs `batch_post_move_evaluate`; `checker_play` vs `batch_checker_play` with `force_boards`).

---

- **ID & Title:** `[NG-PY-04] Conflicting and misdocumented jacoby/beaver defaults across Python, pybind11, C++, simulation, and cube_points`
- **Severity / Effort / Pillar:** `P1` · `S` · `Architecture`
- **Evidence:** C++ `CubeInfo` (`cube.h:27-28`) defaults `jacoby = false, beaver = false`. `CLAUDE.md:66` and `docs/api.md:226-228` state C++ bindings default both to `false`, yet `bindings.cpp:4796-4797, 4841-4842, 5488, 5532, 5591` default `jacoby = true, beaver = true` (while `cl2cf_money` at `2799` defaults `false`). `batch_checker_play` (`batch.py:488`) takes `jacoby=True` without `beaver` (implicitly hitting C++ `true`), `cube_points.py:493-494` defaults `jacoby=True, beaver=False`, and `simulation.py:177` / `puretd.py:179` hardcode `beaver=False`.
- **Impact:** Callers calling `bgbot_cpp` or `batch_checker_play` can silently inherit mismatched `beaver` rules relative to documentation or sibling modules.
- **Recommendation:** Preserve public Python defaults (`jacoby=True, beaver=True` on analyzer/batch, auto-off in match play), pass `beaver` explicitly in `batch.py`, add optional `CubeRules` value objects, clarify simulation no-beaver behavior, and correct `CLAUDE.md:66` & `docs/api.md:226-228`.
- **Verification & Risk Guardrail:** Extend `test_batch_mixed_rules.py` and `test_cube_points.py`.

---

- **ID & Title:** `[NG-PY-05] Missing Policy Protocol & public hooks force worker.py into private internals and duplicate weights in DifficultyPolicy`
- **Severity / Effort / Pillar:** `P1` · `M` · `Extensibility | Performance`
- **Evidence:** NeonDojo `worker.py:80, 122-163, 197-214` inspects `self.analyzer._analyzer._inner._strategy_1ply`, `_FLIP_OWNER`, `FILTER_*`, `_contact_cube_*`, calls `self.native.cubeful_probs_and_equity_nply`, mutates `policy._rng.seed(seed)`, and bypasses `DifficultyPolicy` at 3-ply `full_strength` (leaving `opponent_cube` at 2-ply). `difficulty.py:66-74` constructs a private `BgBotAnalyzer` per instance; `simulation.py:129-136` rebuilds default policies on every `simulate_game()` call.
- **Impact:** Downstream breakage on internal renames, multiplied 24-NN RAM in NeonDojo Play workers, 3-ply `FullStrength` checker/cube asymmetry, and slow default simulations.
- **Recommendation:** Define a `Policy` `Protocol`; allow `DifficultyPolicy(..., analyzer=..., eval_level=...)` + `reseed(seed)`; expose public `top_two_promote`, `resolve_owner`, `flip_owner`, and `contact_cube_life` on `BgBotAnalyzer`; add explicit `default_simulation_policies()` helper.
- **Verification & Risk Guardrail:** `test_difficulty.py`, `test_puretd.py`, `test_simulation.py`, and NeonDojo `worker.py` tests asserting zero private attribute usage.

---

- **ID & Title:** `[NG-PY-06] MLX training pipeline pays per-epoch host syncs, disk weight serialization, and 32× chunk recompilations`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance | Architecture`
- **Evidence:** `sl_train.py:98-107, 178-196` constructs `NeuralNetwork` via temp `.weights` disk write/parse, uploads `rng.permutation` every epoch, and on benchmark hooks executes `write_weights(bench_tmp, *host_weights())` + `bgbot_cpp.score_benchmarks_nn` disk reload **every epoch**. `scripts/run_s11_containment_sl.py:74-81, 160-175` chunks 2,500-row slices (32× `_make_step` recompiles) and scores holdout ER in Python loops.
- **Impact:** Unnecessary GPU→CPU→disk→C++ overhead and slow experiment iteration on M4 Pro.
- **Recommendation:** Init MLX weights directly in NumPy matching Xavier parity, keep permutation on-device, score validation in MLX (or pass in-memory buffers to C++), and add epoch callbacks to `supervised_train_preencoded`.
- **Verification & Risk Guardrail:** Preserve `tests/test_sl_train.py` `2e-5` float64 C++ parity.

---

- **ID & Title:** `[NG-PY-07] Packaging, wheel asset bloat, and MODELS registry drift across pyproject.toml, CMakeLists.txt, and weights.py`
- **Severity / Effort / Pillar:** `P1` · `S` · `Architecture`
- **Evidence:** `pyproject.toml:72-77` and `CMakeLists.txt:37-40` bundle 27 `.weights.best` files (10.24 MB), yet `stage11` uses 21 distinct files — shipping **6 unused Stage 9 files (2.08 MB)**. `pyproject.toml` lacks `CMakeLists.txt:51`'s `_tdinit` exclusion and declares no `[project.optional-dependencies]`. `weights.py:23-120` advertises 8 dev-only stages (`stage3`..`stage8`, `stage10`) absent from wheels despite `docs/api.md` examples (`"stage5"`, `"stage3"`) and mistypes `WeightConfig.from_args`.
- **Impact:** ~2 MB wasted per wheel/sdist, sdist vs wheel skew, guaranteed `FileNotFoundError` for pip users following docs, missing optional extras (`train`, `cloud`, `dev`).
- **Recommendation:** Derive wheel/sdist globs from shipped model manifests (dropping the 6 unused `sl_s9_*` weights), add `_tdinit` exclude & optional extras in `pyproject.toml`, tag checkout-only models explicitly, update `docs/api.md` examples to `stage11`, and fix `from_args` return type.
- **Verification & Risk Guardrail:** Wheel/sdist build + install smoke test checking all manifest weights exist and no orphaned weights are packed.

---

- **ID & Title:** `[NG-PY-08] __init__.py eager C++ imports break text_export.py pure-Python usage; data.py mutates sys.path; legacy BackgammonSage string`
- **Severity / Effort / Pillar:** `P2` · `S` · `Architecture`
- **Evidence:** `python/neongammon/__init__.py:18-52` eagerly imports `bgbot_cpp`-dependent modules, causing `import neongammon.text_export` (documented at `text_export.py:39-42` as pure Python) to fail with `ModuleNotFoundError: No module named 'bgbot_cpp'` when unbuilt (verified directly). `text_export.py:516` emits `[Site "BackgammonSage"]` and `_is_die` (`30-31`) accepts `bool`. `data.py:12-15` mutates `sys.path` at import time.
- **Impact:** Pure-Python notation/XG/settings tools cannot run without compiled C++; hidden `sys.path` hacks mask install errors; stale site branding in exports.
- **Recommendation:** Use PEP 562 lazy `__getattr__` in `__init__.py` (`TYPE_CHECKING` preserved), remove `sys.path` injection in `data.py`, reject `bool` in `_is_die`, and update `[Site "NeonGammon"]`.
- **Verification & Risk Guardrail:** Test importing `neongammon.{text_export,xg_file,settings}` with `bgbot_cpp` blocked; add `_is_die(True) is False` check to `test_move_notation.py`.

---

- **ID & Title:** `[NG-PY-09] Duplication across xg_compare.py vs xg_file.py, board flip helpers, gnubg.py, and cube_points.py`
- **Severity / Effort / Pillar:** `P2` · `M` · `Modularity`
- **Evidence:** `xg_compare.py` (976 LOC) reimplements `xg_file.py` (590 LOC) binary archive/record parsing without CRC-32 verification; board flip & prob inversion appear in 4+ modules (`board.py`, `data.py:213-221`, `gnubg.py:46-53`, `xg_compare.py:86-90`); `gnubg.py:572-608` spawns a subprocess per call and sorts `checker_play` by `-r['equity_cubeless']`; `cube_points.py` emits `NaN` floats rather than `None`.
- **Impact:** Parser fixes diverge; `GnuBgAnalyzer` vs `BgBotAnalyzer` ranking semantics differ silently; `NaN` breaks JSON consumers.
- **Recommendation:** Make `xg_compare.py` consume `xg_file.py`; standardize perspective flips on `board.py`; align/document `GnuBgAnalyzer` cubeful vs cubeless contract; replace `NaN` with `float | None` in `cube_points.py`.
- **Verification & Risk Guardrail:** `test_xg_file.py`, `test_gnubg.py`, `test_cube_points.py`, perspective round-trip tests.

---

- **ID & Title:** `[NG-PY-10] Incomplete public type annotations, missing py.typed marker, and KeyError vs ValueError mismatch`
- **Severity / Effort / Pillar:** `P2` · `M` · `Extensibility`
- **Evidence:** No `py.typed` file exists. Public functions in `gnubg.py`, `matchinfo.py`, `simulation.py`, `move_arrays.py`, `sl_train.py`, and `analyzer.py` lack full annotations; batch APIs accept untyped `list[dict]`; `resolve_owner` (`analyzer.py:59-63`) raises `KeyError` on unknown values unlike sibling `ValueError` checks.
- **Impact:** Type checkers treat `neongammon` as `Any`; batch callers get zero key checking; inconsistent validation exceptions.
- **Recommendation:** Add `python/neongammon/py.typed`, `TypedDict` schemas for batch inputs, `Sequence[int]` (`BoardLike`) annotations, full module typing, and `ValueError` in `resolve_owner`.
- **Verification & Risk Guardrail:** `mypy` / `pyright` CI step + `pytest`.

---

- **ID & Title:** `[NG-PY-11] Research script sprawl in scripts/ (146 files): 52 broken path shims, ../refpos.txt, duplicate helpers, and legacy AWS worker`
- **Severity / Effort / Pillar:** `P2` · `M` · `Modularity`
- **Evidence:** `scripts/` contains 146 `.py` files (`42` `run_*`, `29` `bench*`, `8` standalone `test_*.py`): 52 still append nonexistent `neongammon/python` paths, 8 hardcode `../refpos.txt`, 98 bypass `neongammon` for `bgbot_cpp`, `load_rollout`/`parse_refpos`/`flip_board` are copy-pasted across 6–8 scripts, and `ecs_rollout_worker.py` (AWS Stage 5) sits beside `modal_rollout.py`.
- **Impact:** Broken scripts for contributors and blurred boundary between supported library and archived experiments.
- **Recommendation:** Standardize `pip install -e .[train,dev]`, clean stale `sys.path` shims, move active `test_*.py` into `tests/`, extract `scripts/_common.py`, and archive completed Stage 3–10 scripts + `ecs_rollout_worker.py` under `scripts/archive/` (preserving `scripts.calibrate_difficulty` imports).
- **Verification & Risk Guardrail:** `tests/test_difficulty_calibration.py` + `py_compile` across `scripts/`.

---

- **ID & Title:** `[NG-PY-12] tests/ coverage gaps (luck, matchinfo, text_export, xg_compare, DifficultyPolicy sampling) and stale isolation note`
- **Severity / Effort / Pillar:** `P2` · `M` · `Architecture`
- **Evidence:** `tests/` (41 files, 8,184 LOC) has 0 occurrences of `roll_luck` / `luck_from_equities`, `matchinfo`, `export_history_to_txt`, `load_gnubg_training_data`, and synthetic `xg_compare.py` PR tests; `test_checker_cube_symmetry.py:21` hardcodes `../../build`; `test_vr_and_dead_cube.py:31-45` documents 4 ordering failures after `test_2ply_details.py` that no longer fail empirically (46/46 passed in our run).
- **Impact:** Regressions in pure-Python PR/luck/text-export calculations lack automated coverage; stale failure docs mislead contributors.
- **Recommendation:** Add synthetic fixture tests for `luck.py`, `matchinfo.py`, `text_export.py`, `xg_compare.py` PR math, and `DifficultyPolicy` sampling distribution; fix `test_checker_cube_symmetry.py:21`; update stale note in `test_vr_and_dead_cube.py` after fixing `[NG-CPP-04]` `cache_salt_`.
- **Verification & Risk Guardrail:** Order-independent full `pytest tests/` pass.

---

### 4.3 NeonDojo Rust Backend Architecture (`backend/src/`, `shared/`)

- **ID & Title:** `[ND-BE-01] Sealed two-engine (gnubg/bgsage) assumption leaks across 12+ modules and zeroes NeonGammon Play rating_decisions`
- **Severity / Effort / Pillar:** `P1` · `L` · `Extensibility | Architecture`
- **Evidence:** `engines/mod.rs:645-759` (`AnalysisEngine::Gnubg | OpenSage`) abstracts only analysis, omitting cube, play turns, top-2, presets, and resignation. Engine variants/IDs leak into `api.rs:695-709`, `configuration.rs:27`, `settings.rs:166-200`, `play.rs`, `play/feedback.rs:351-375`, `practice/evaluation.rs:114-128, 370-470`, `similar_positions.rs:288-305` (ignores `settings.analysis_engine` and calls `settings.gnubg()` unconditionally!), `review_decisions.rs:882-887`, `match_jobs.rs:88-93`, `engine_matches.rs:355-388`, `benchmarks.rs`, `analysis_timing.rs`, and `databases.rs:518`. Critically, `play/open_sage.rs:420, 442, 481` hardcodes `rating_decisions: 0` on all NeonGammon `PlayAlternative`s (unlike `play/feedback.rs:404, 466`), silently zeroing rated decision counts and `relevance_delta` in Play/Study!
- **Impact:** Adding a 3rd engine requires 12+ cross-cutting edits; NeonGammon Play/Study PR statistics silently corrupt `rating_decisions`; `similar_positions::score` fails when only NeonGammon is configured.
- **Recommendation:** Fix `play/open_sage.rs` `rating_decisions` & route `similar_positions::score` via `settings.analysis_engine(...)` immediately (Phase A); in Phase B introduce an `EngineProvider` capability trait/registry in `engines/` with tagged JSON `ProviderConfig` inside `EngineSnapshot` (keeping `"bgsage"` DB IDs and `"NeonGammon"` display mapping in one place).
- **Verification & Risk Guardrail:** Guard XG 1-ply convention, no silent fallback, and `"bgsage"` DB compatibility via `cargo test --workspace` + `npm run api:types --prefix web`.

---

- **ID & Title:** `[ND-BE-02] Mega-modules tangle HTTP/OpenAPI handlers, domain logic, and raw SQL/QueryBuilders`
- **Severity / Effort / Pillar:** `P1` · `L` · `Modularity | Architecture`
- **Evidence:** Seven modules exceed 1,000 LOC: `play.rs` (1,899 LOC beside `play/`), `databases.rs` (1,812 LOC, including 590-LOC `browse` & N+1 per-decision `move_analyses` inserts in `copy_children` at `1436-1440`), `opening_book.rs` (1,740 LOC), `learning/mod.rs` (1,447 LOC), `match_analysis.rs` (1,395 LOC), `review_decisions.rs` (1,038 LOC), and `position_browser.rs` (1,021 LOC). Also `profiles.rs:89-99` runs an N+1 query loop per profile ID, and `analysis_statistics/catalog.rs:98-182` reads sequentially outside a snapshot transaction.
- **Impact:** High merge friction, coupled transport/persistence logic, N+1 latency in `profiles.rs` and `copy_children`, and unwieldy `api.rs` OpenAPI wiring.
- **Recommendation:** Split oversized domains into `<domain>/{routes,service,repo}.rs` (`play/`, `databases/`, `opening_book/`, `learning/`, `match_analysis/`), load profiles in one query in `profiles.rs`, read `catalog.rs` inside `REPEATABLE READ READ ONLY`, and batch `copy_children` (see `[ND-SEAM-05]`).
- **Verification & Risk Guardrail:** Preserve REST paths, HTTP statuses, cursors, and OpenAPI via `cargo test --workspace` + `npm run api:types --prefix web`.

---

- **ID & Title:** `[ND-BE-03] Fragmented HTTP error contracts & Axum extractor rejections (plain-text 400/422 leaks + dual ConfigurationErrorBody)`
- **Severity / Effort / Pillar:** `P1` · `M` · `Architecture`
- **Evidence:** `api.rs:81-102` standardizes `ApiError -> ErrorBody`, yet 4 modules write private rejection helpers (`databases.rs:39-44`, cross-imported by `database_backup.rs:419`; `position_browser.rs:353-362`; `learning/mod.rs:50-63`; `configuration.rs:455`), while raw `Json`/`Query` return Axum `text/plain` rejections on `play.rs:716, 839, 1187, 1246, 1481, 1568`, `analysis_catalog/mod.rs:319`, `study.rs:84, 225`, `engine_matches.rs:281, 424`, `match_assessment/mod.rs:190, 205-206`, and `api.rs:283, 441, 561, 629`. `configuration.rs:368-454` maintains a parallel `ConfigurationErrorBody { error, code, issues }` bridged via `legacy_error()`.
- **Impact:** Play/Study/EngineMatch clients receive unparseable `text/plain` validation errors instead of localized JSON; configuration uses bespoke frontend error wrappers.
- **Recommendation:** Define shared `ApiJson<T>` / `ApiQuery<T>` extractors in `api.rs`, delete all 4 private helpers, and add optional `code` and `issues` onto `ErrorBody` so `ConfigurationError` maps natively into `ApiError`.
- **Verification & Risk Guardrail:** `cargo test --workspace`, `npm run api:types --prefix web`, and frontend `api.ts` error tests.

---

- **ID & Title:** `[ND-BE-04] Duplicated GNUbg Position/Match ID codecs across 6 modules & core checker rules trapped inside import::sgf`
- **Severity / Effort / Pillar:** `P1` · `M` · `Architecture | Extensibility`
- **Evidence:** `import/ids.rs` owns `MatchId`/`position_id`, yet `learning/apkg.rs:307-325` (`match_fields`), `learning/content.rs:532-563` (`decode_board`), `cube_context.rs:135-144, 178`, and `opening_book.rs:566-573, 1038-1123` reimplement Base64/bit-unpacking. Legal checker rules (`CheckerPath`, `checker_paths`, `apply_checker_turn`, `expand_xg_play`) live in `import/sgf.rs`, forcing `play.rs`, `practice.rs`, and `move_assessments.rs` to depend on `crate::import::sgf`.
- **Impact:** Perspective/bit-field fixes require multi-file edits; domain logic depends backwards on an import parser.
- **Recommendation:** Extract a pure `backgammon/` module owning `Board`, `GnubgPositionId`, `GnubgMatchId`, and legal checker turn execution; delete duplicated codecs in `apkg.rs`, `content.rs`, and `opening_book.rs`.
- **Verification & Risk Guardrail:** Unit/integration tests across `import::ids`, `cube_context`, `opening_book`, `learning::apkg`, `practice`, and `play`.

---

- **ID & Title:** `[ND-BE-05] Single-source-of-truth drift in loss rounding, ACTIVE_COMPETITION SQL, and config validation`
- **Severity / Effort / Pillar:** `P1` · `M` · `Architecture | Correctness`
- **Evidence:** Equity loss rounding (`normalized_loss`) is duplicated across `match_analysis.rs`, `move_assessments::rounded_loss`, `play/open_sage.rs:446, 489`, and omitted in `recurring_mistakes/imported_projection.rs:297-300`. Unfinished competition exclusion SQL (`ACTIVE_COMPETITION`) is copy-pasted across 6 files (`play.rs:497-500`, `position_browser.rs:436`, `similar_positions.rs:29-32`, `databases.rs`, `recurring_mistakes/{source,index}.rs`). `configuration/validation.rs:161-163` re-parses `shared/configuration-options.json` on every `normalize()` call instead of sharing `configuration::OPTIONS`, `configuration/store.rs:258-261` skips engine validation when only `config.play.engine` changes, legacy `app_settings` tables remain dual-written, and `learning/exercises.rs:69-73` repeats `if row.is_anki()`.
- **Impact:** Changing `play.engine` alone bypasses installation validation; duplicated competition SQL risks visibility skew; wasted JSON parsing per request.
- **Recommendation:** Centralize `normalized_loss`, move `ACTIVE_COMPETITION` into a shared DB view or SQL constant, reuse `configuration::OPTIONS`, validate `config.play.engine != old.play.engine`, retire legacy `settings.rs` write paths once unified, and remove duplicate `is_anki()`.
- **Verification & Risk Guardrail:** `cargo test --workspace` (configuration, play visibility, recurring_mistakes, imported_projection).

---

- **ID & Title:** `[ND-BE-06]` (P2 · M · Extensibility | Modularity) **Hardcoded match import/export format dispatch in HTTP transport:** `api.rs:317-384` (`upload`) hardcodes extensions and runs `Engine::import_mat` directly; `databases.rs:1477-1525`, `import/match_context.rs`, `opening_book.rs`, and `recurring_mistakes/` scatter format string switches. **Fix:** Introduce `import::FormatRegistry` (`MatchImporter`/`MatchExporter` traits) outside `api.rs`. **Verify:** `import_*` and upload/export tests.
- **ID & Title:** `[ND-BE-07]` (P2 · M · Architecture | Testability) **Process-wide mutable static locks/caches bypass `AppState`:** `practice.rs:13` (`EVALUATION: Mutex` locked by `similar_positions.rs:285`), `play/open_sage.rs:19`, `learning/anki_*.rs`, `xg_book.rs:452`, `opening_book.rs:589`, `engine.rs:10`. **Fix:** Move runtime semaphores, evaluation gate, and bounded LRU book caches onto `AppState`/`RuntimeServices` for concurrency isolation and clean Tauri restarts. **Verify:** Preserve concurrency caps + `cargo test --workspace`.
- **ID & Title:** `[ND-BE-08]` (P2 · M · Extensibility) **Study/exercise extensibility constrained by untyped `CardContent` JSONB & `LOCK TABLE`:** `learning/mod.rs` / `exercises.rs` / `rounds.rs` / `practice/evaluation.rs` switch on string card kinds alongside 2.6k LOC of interleaved Anki sync (`anki_*.rs`, `apkg.rs`), and `learning/transfer.rs:50` takes `LOCK TABLE transfer_practice_seen IN SHARE ROW EXCLUSIVE MODE`. **Fix:** Strongly type `ExerciseKind`, isolate `learning/anki/*`, keep shared `card_signature` + Check-vs-Rate separation, and replace `LOCK TABLE` with `pg_advisory_xact_lock`. **Verify:** `learning`/`practice` integration suites.
- **ID & Title:** `[ND-BE-09]` (P2 · S · Architecture | Reliability) **Non-test `unwrap()`/`expect()` panics and `main.rs` bootstrap coupling:** Runtime unwraps in `learning/search.rs:372,387,401`, `play.rs:1526,1779`, `databases.rs:654,661,1259,1573`, `recurring_mistakes.rs:325`, `study.rs:173`, `benchmarks/estimate.rs:266-276`; `main.rs:22-85` inline-owns DB/migration/backfill/worker/CORS setup. **Fix:** Return `.context(...)?` and extract `neondojo::runtime::start(RuntimeConfig)` for embedded Tauri hosting. **Verify:** Clippy + Tauri lifecycle smoke test.

---

### 4.4 NeonDojo PostgreSQL, Jobs & Engine Seam (`migrations/`, `engines/`, `jobs.rs`, SQL hot paths)

*(Note: `[ND-SEAM-01]` through `[ND-SEAM-04]` covering `verify()`, `analyze_rolls`/`SageEngine::game` batching, M4 Pro `CPU_BUDGET` + `ENGINE_WORK.write()` starvation, and `worker.py` private hooks are detailed in Section 3 above; database, jobs, backup, and Tokio findings follow.)*

- **ID & Title:** `[ND-SEAM-05] played_record_fingerprint trigger write amplification (O(N²) in copy_children) and missing duplicate-search index`
- **Severity / Effort / Pillar:** `P1` · `S` · `Performance`
- **Evidence:** `0086_played_record_fingerprint.sql:13-93` recomputes SHA-256 across all positions of a match per `INSERT` statement on `positions`; `databases.rs:1389-1441` (`copy_children`) inserts copied `positions`, `match_games`, `match_moves`, `annotations`, and `move_analyses` **1 row per SQL round-trip in `for` loops** (firing the trigger N times = `O(N²)` hashes!). Meanwhile `duplicate_matches.rs:126-145` groups on `(match_length, played_record_fingerprint)` with no index.
- **Impact:** Match copying scales quadratically in CPU/DB writes; duplicate detection scans `matches` sequentially at 10k+ matches.
- **Recommendation:** Batch all `copy_children` inserts with `INSERT ... SELECT` / `QueryBuilder` (reducing N trigger fires to 1 or copying `played_record_fingerprint` directly) and add partial index `matches_played_record_idx ON matches (match_length, played_record_fingerprint) WHERE NOT collection_only AND played_record_fingerprint IS NOT NULL`.
- **Verification & Risk Guardrail:** `EXPLAIN (ANALYZE, BUFFERS)` on `duplicate_matches.rs:127-146` in a 10k-match test DB; copy benchmark on a 25-point match preserving identical fingerprint.

---

- **ID & Title:** `[ND-SEAM-06] similar_positions.rs streams entire positions table into Rust; missing indexes in position_browser, recurring_mistakes, and databases`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance`
- **Evidence:** `similar_positions.rs:155-212` streams every non-blocked row in `positions` (`500k+`) to compute distances in Rust. `position_browser.rs:573-647` filters on unindexed `coalesce(m.source_database_id, m.database_id) = $1` and computes `effective_analysis_selections` / `learning_cards` joins prior to `LIMIT` + `OFFSET` + `count(*)`. `recurring_mistakes/search.rs:250-290` runs `OFFSET` + `count(*)` over `decision_index` where `(player_name, equity_loss DESC)` misses selective `question_kind`/`game_plan`/`database_id` filters. `databases.rs:1156-1158` joins `jobs` without a partial active `position_uuid` index, and `jobs_claim` indexes completed/failed rows.
- **Impact:** Linear degradation at 500k+ positions / 10k+ matches and pool connection pinning during similarity searches.
- **Recommendation:** Add stored/generated coarse feature columns (`pip_own`, `pip_opp`, `borne_off_own`, `borne_off_opp`, `bar_own`, `bar_opp`, `is_contact`) + B-tree index on `positions` for SQL bounding-box pre-filtering (widening only if `<50` hits); add `matches(coalesce(source_database_id, database_id))`, `jobs_queued_idx`, and `jobs_active_pos_idx` partial indexes; defer post-pagination lateral joins in `position_browser.rs`; add selective composite indexes on `decision_index` (preserving `DecisionSearch`'s `NumberedSearchPagination` early pages unless cursor UI is explicitly chosen).
- **Verification & Risk Guardrail:** Seed isolated 10k-match / 500k-position test DB and capture `EXPLAIN (ANALYZE, BUFFERS)` + top-50 brute-force parity test.

---

- **ID & Title:** `[ND-SEAM-07] Backup restore leaves 0086/0089/0090 triggers armed during COPY, collides with multi-hour job locks, and hardcodes allowlists`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance | Architecture | Extensibility`
- **Evidence:** `database_backup/restore.rs:27-32, 66-71` disables only `catalog_match_players`, leaving `0086` fingerprint re-hashing, `0089` revision triggers (`0089_decision_source_revisions.sql:80-105`), and `0090` `pg_notify` SSE triggers (`0090_job_event_notifications.sql:1-15`) armed during bulk `COPY FROM STDIN`. `jobs.rs:84-94` holds `backup.requests` read guard across multi-hour runs, blocking restore's `write()` (`lock_timeout = 3s`, HTTP 409). `database_backup.rs:35-45` requires manual `ARCHIVE_COMPATIBLE_MIGRATIONS` allowlist updates.
- **Impact:** Restore triggers massive SHA-256/WAL/`pg_notify` storms and returns 409 whenever any analysis job runs.
- **Recommendation:** Disable derived/notification user triggers (keeping FKs/constraints enabled) during `copy_tables` and rebuild derived tables on demand; replace job-duration `backup.requests` locks with short DB-transaction scoping + cooperative job pause/cancel on restore.
- **Verification & Risk Guardrail:** Restore a 10k-match / 100k-`job_events` archive in a test DB with an active SSE subscriber verifying zero restore `pg_notify` emissions and passing `database_backup` tests.

---

- **ID & Title:** `[ND-SEAM-08] PostgreSQL advisory lock ID collision (91432302) between jobs::claim and engine_matches::claim`
- **Severity / Effort / Pillar:** `P1` · `S` · `Performance | Architecture`
- **Evidence:** Both `jobs.rs:119` (`SELECT pg_advisory_xact_lock(91432302)`) and `engine_matches/runner.rs:65` (`SELECT pg_try_advisory_xact_lock(91432302)`) use the exact same integer key.
- **Impact:** `jobs::claim` holding `91432302` across lease recovery/event inserts causes `engine_matches::runner::claim` to fail `pg_try_advisory_xact_lock` and skip cycles.
- **Recommendation:** Assign distinct advisory lock keys (or 2-arg `pg_advisory_xact_lock(domain, id)`) from a shared registry constant.
- **Verification & Risk Guardrail:** Integration test holding `jobs::claim` lock while claiming an `engine_match_runs` row concurrently.

---

- **ID & Title:** `[ND-SEAM-09] Under-sized PgPool (max_connections=10, acquire_timeout=5s), no analysis job cancel route, and unpruned job_events`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance | Extensibility`
- **Evidence:** `main.rs:22-26` configures `.max_connections(10)` + `.acquire_timeout(5s)` for up to 8 concurrent analysis jobs (`parallel_analysis_jobs`), 3 background workers, SSE replay, and heavy UI reads. `api.rs:154-160` + `analysis_catalog/mod.rs:285` lack `/api/jobs/{id}/cancel` (unlike benchmarks/duels), and `job_events` (`0001_schema.sql`) never prunes progress rows.
- **Impact:** Pool starvation (`acquire_timeout` failures / false lease expiry) under concurrent analysis + UI load; runaway jobs cannot be cancelled cleanly from the UI; unbounded `job_events` table growth.
- **Recommendation:** Raise `max_connections` (e.g., `24–32`), add `POST /api/jobs/{id}/cancel` aborting the active `JoinSet` task so `kill_on_drop` reaps child workers, and compact non-terminal `progress` rows on completion/retention.
- **Verification & Risk Guardrail:** Stress test 8 parallel jobs + SSE + UI reads checking `acquire_timeout == 0` and clean subprocess termination on cancel.

---

- **ID & Title:** `[ND-SEAM-10]` (P2 · S · Performance | Architecture) **Synchronous SGF parsing & `$PATH` probing on Tokio worker threads:** `store.rs:685` parses SGF inside an open `FOR SHARE` DB transaction; `match_analysis.rs:176-218` & `engines/game.rs:54` replay SGF synchronously on Tokio threads; `engine.rs:43-58,374` scans `$PATH` via blocking `std::fs`. **Fix:** Offload onto `tokio::task::spawn_blocking` (outside DB transactions). **Verify:** `match_analysis` / `jobs` integration tests.

---

### 4.5 NeonDojo Frontend Architecture & Performance (`web/src/`, `web/vite.config.*`)

- **ID & Title:** `[ND-FE-01] Flat 427-file web/src/ layout, mega-components up to 2,497 LOC, and 5× duplicated database fetching`
- **Severity / Effort / Pillar:** `P1` · `L` · `Modularity`
- **Evidence:** `web/src/` places 427 files (108,375 LOC) in one flat directory with mega-components `PlayWorkspace.tsx` (2,497 LOC), `SettingsDialog.tsx` (1,715), `LearningPanel.tsx` (1,633), `App.tsx` (1,359, drilling ~25–30 props), `MatchDatabase.tsx` (1,281), `EngineMatchWorkspace.tsx` (1,113), `AnalysisWorkspace.tsx` (1,108), `MatchAnalysisPanel.tsx` (851), and `AnalysisPanel.tsx` (778). Five workspaces independently duplicate `useDatabases()` (`MatchDatabase.tsx:287`, `PositionBrowser.tsx:134`, `DecisionSearch.tsx:159`, `PlayerDirectory.tsx:201`, `RecurringMistakes.tsx:128`).
- **Impact:** Merge friction, heavy coupling through `App.tsx`, and hidden inactive workspace trees re-rendering on root changes.
- **Recommendation:** Reorganize into feature slices (`src/{app,shared,settings,workspaces/{play,analysis,study,positions,library,players,extras}}/`), extract `useDatabases()`, and split `PlayWorkspace`, `SettingsDialog`, and `LearningPanel` (`AddCardsPanel`, `DeckSelectorBar`, `AnkiActionsMenu`, `StudyQueueView`).
- **Verification & Risk Guardrail:** Keep `App.tsx` `views` order & `workspace-tab-*` IDs; verify `npm test`, `typecheck:e2e`, `build`, and `text-navigation.spec.ts`.

---

- **ID & Title:** `[ND-FE-02] Monolithic 1.4 MB JS / 202 KB CSS bundle, zero code splitting, eager 598 KB 6-locale i18n, and startup catalog validation`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance`
- **Evidence:** Single `web/dist/assets/` JS chunk is **1.4 MB** (`202 KB` CSS) with no `manualChunks` (`vite.config.ts`). `App.tsx:10-40` statically imports all 7 workspaces + `SettingsDialog`, all 40 i18n files (**598.2 KB** raw across 6 locales), and `generated-themes.json` (**65.3 KB**). `i18n-core.ts:241` runs `validateCatalogs()` synchronously on every page load.
- **Impact:** Every startup parses all 6 locales, 13 BGBlitz theme manifests, and all idle workspaces before first paint.
- **Recommendation:** `React.lazy` non-initial workspaces + `SettingsDialog` (keeping visited workspaces mounted on first visit so tab switches preserve live Play/Analysis state), split i18n into lazy per-locale chunks, lazy-load `generated-themes.json`, and run `validateCatalogs()` in Vitest / dev only.
- **Verification & Risk Guardrail:** Measure built asset chunks; run `npm test --prefix web`, `ui-themes.spec.ts`, `typography.spec.ts`.

---

- **ID & Title:** `[ND-FE-03] Inverted CSS import order in main.tsx, specificity escalation, and ~400+ LOC dead CSS in style.css`
- **Severity / Effort / Pillar:** `P1` · `M` · `Architecture`
- **Evidence:** `web/src/main.tsx:3-9` imports `App` **before** `./style.css`, `./desktop-analysis.css`, `./typography.css`, `./text-navigation.css`, placing 38 component CSS files *earlier* in cascade order than `style.css` and triggering `.app-shell`/`.analysis-workspace` nesting and `!important` overrides (`workspace.css:6,407`). `style.css` (`1,902 LOC`) retains ~400+ lines of dead sidebar/welcome rules (`101-385, 902-988, 1047-1338, 1532-1584`).
- **Impact:** Styles fight `style.css` unpredictably; 202 KB stylesheet carries dead layout rules.
- **Recommendation:** Fix `[ND-UX-06]` tokens first, then reorder `main.tsx` (`ui-appearance.css`, `typography.css`, base reset before `App`, followed by `desktop-analysis.css` and `text-navigation.css` per `docs/ui-navigation.md:28`), and prune dead selectors from `style.css`.
- **Verification & Risk Guardrail:** `text-navigation.spec.ts`, `typography.spec.ts`, `ui-themes.spec.ts` at `1800×1169`, `2560×1440`, `3840×2160`.

---

- **ID & Title:** `[ND-FE-04] High-frequency re-renders: 65 ms dice ticks breaking PlayHistory memo, 120 Hz drags, uncached Intl, and OrderedLibraryRow churn`
- **Severity / Effort / Pillar:** `P1` · `M` · `Performance`
- **Evidence:** `usePlayAnimation.ts:56-65,100-112` ticks every 65 ms re-rendering all of `PlayWorkspace.tsx` (which also computes `applySteps` twice per render at `905, 2089` and breaks `PlayHistory` `memo()` via unmemoized `onSelect`/inline `reviewAction` at `1774-1802`). `useAnalysisBoardResize.tsx:204` & `useResizableColumns.tsx:175` call `setState` on every `pointermove`. `i18n.tsx:28-41` & `i18n-core.ts:167,211` instantiate `new Intl.NumberFormat` on every cell render (plus `new Intl.DateTimeFormat` in 20+ components). `OrderedLibraryRow` (`MatchDatabase.tsx:1260-1280`) runs `Children.toArray` + `.indexOf` 51× per render alongside 51 `<Tooltip as="tr">` hooks (`1276`).
- **Impact:** Dropped frames during dice rolls and 4K pane/column drags; GC/CPU churn across dense tables and 36-cell dice grids.
- **Recommendation:** Isolate dice-roll state + stabilize `PlayHistory` callbacks; update splitter/column CSS custom properties via `requestAnimationFrame` on `pointermove` and commit React state on `pointerup`; cache `Intl.NumberFormat`/`DateTimeFormat` in a Map; replace `OrderedLibraryRow` `Children.toArray` with a column-cell descriptor map.
- **Verification & Risk Guardrail:** Flamegraph during dice roll & splitter drag; `dice-equity.spec.ts`, `play-full-match.spec.ts`, `npm test --prefix web`.

---

- **ID & Title:** `[ND-FE-05]` (P2 · M · Architecture) **Split configuration state & always-mounted `SettingsDialog`:** `App.tsx` merges `useConfirmedConfiguration()`, legacy `loadSettings()`, and `localStorage`; `SettingsDialog.tsx` stays mounted even when closed (`App.tsx:1334-1356`). **Fix:** Mount `SettingsDialog` conditionally while open and retire the `configuration-settings.ts` bridge once installation vs profile APIs split cleanly. **Verify:** `settings.spec.ts`, `configuration-store.test.ts`.
- **ID & Title:** `[ND-FE-06]` (P2 · M · Modularity) **Fragile 4-target `createPortal` + `MutationObserver` in Analysis:** `AnalysisWorkspace.tsx:163-171` holds 4 DOM nodes in `useState` and `MatchAnalysisPanel.tsx:165-174` uses a `MutationObserver` to close popovers on dialog exit. **Fix:** Replace with declarative `useMatchAnalysisController` + `onCalculationStarted` callback (simultaneously unblocking whole-match `Analyze match …` in Analysis). **Verify:** `analysis-position-actions.spec.ts`, `dice-equity.spec.ts`.
- **ID & Title:** `[ND-FE-07]` (P2 · S · Performance) **Redundant polling timers alongside `job-watch.ts` SSE:** `BenchmarkWorkspace.tsx:106-134` polls every 1 s forever even when `!result.active`; `App.tsx:131-150` polls `/api/benchmarks/status` every 2 s globally; `MatchDatabase.tsx:506-516` (4 s) and `useAnalysisCatalog.ts:95-98` (3 s) poll despite SSE `job-watch.ts` transitions. **Fix:** Stop idle benchmark polling and drive MatchDB/catalog refreshes from `job-watch.ts` terminal events. **Verify:** `analysis-calculation-controls.spec.ts`, `engine-matches.spec.ts`.
- **ID & Title:** `[ND-FE-08]` (P2 · M · Extensibility) **Fragmented i18n across 40 files with 3 patterns & silent fallback:** 14 files use `formatMessage`, 11 use unlocalized `interpolate`, 14 use raw dicts (`play-messages.ts` `Object.assign` + `?? en[key]` bypassing `validateCatalogs`). **Fix:** Unify on one typed `defineMessages` + memoized `formatMessage(locale, ...)` tested across all 40 catalogs and all 6 languages (`EN`, `DE`, `ES`, `FR`, `IT`, `PL`). **Verify:** Vitest plural/placeholder coverage + `typography.spec.ts`.
- **ID & Title:** `[ND-FE-09]` (P2 · S · Architecture) **API contract bypasses in `configuration-api.ts` / `settings-api.ts` / raw `fetch()`:** Bespoke `fetch` wrappers + `ConfigurationApiError`/`SettingsApiError` bypass `api.ts` `AppError` localization and `generated-api.d.ts` compile checks (`ProfileSettingsPanel` renders raw English errors); `LearningPanel.tsx:888` & `position-browser-api.ts:226` call raw `fetch()`. **Fix:** Route all calls through `api.ts` + `generated-api.d.ts` (consuming `[ND-BE-03]`'s extended `ErrorBody`). **Verify:** `api:types`, `typecheck:e2e`, `settings.spec.ts`.
- **ID & Title:** `[ND-FE-10]` (P2 · S · Architecture) **Render-time `setState` and ref mutations:** `App.tsx:119-120`, `PositionBrowser.tsx:175`, `useSearchPages.ts:32-36`, and `LearningPanel.tsx:507-520` mutate state/refs during render. **Fix:** Derive state via pure selectors and move transitions/ref updates into handlers/`useEffect`. **Verify:** StrictMode Vitest + `library-position-results.spec.ts`.

---

### 4.6 NeonDojo UX, Usability & Visual Design (`web/src/*.tsx`, `web/src/*.css`, `docs/`, `web/e2e/`)

> **Live UI Verification Summary (`http://127.0.0.1:8787`, `1800×1169` & `2560×1440`, Graphite Dark & Light):**
> - **Verified Strengths:** Zero horizontal overflow (`1800px` / `2560px`); crisp visual hierarchy on `Study -> Overview` (`1800×1169`) and `Settings -> Appearance` (`Graphite · Dark` live preview badge contrast); clean board arrow rendering and move-difference breakdown on populated `Analysis` (`leihande vs. clioss`, Move 52).
> - **Live-Confirmed Defects:**
>   1. **Match Database column sizing, dim batch bar & tooltip overlay ([final_step_26.png](file:///Users/hendrik/.gemini/antigravity/brain/12492bdd-facf-41b1-bd2f-f8a618ddf2aa/browser_evidence/92e170a7-f527-464b-b78a-ea032f700d19/final_step_26.png)):** Even at `2560×1440`, `Imported on` truncates to `Importe...` and `Play mode` truncates to `Coach · Wi...` / `Competitio...` while `White`, `Black`, and `Result` hold wide empty gaps; disabled batch icons blend dimly into the dark surface; hovering row `ID: 240` pops an opaque row tooltip obscuring rows `239–236`.
>   2. **OS-locale decimal leak in `Positions -> All analyzed decisions` (`1800×1169`):** `Equity loss from` `<input type="number">` displays **`0,05`** (OS comma) directly above dot-formatted table cells (`1.037`, `1.012`, `0.955`).
>   3. **Low dark-mode Play status contrast (`1800×1169`):** `Bot · 0 points` / `167 pips · 0 off` and `You · 0 points` / `167 pips · 0 off` around the board render in dim muted grey on dark backgrounds.
>   4. **Analysis vertical overflow & whitespace (`1800×1169` & `2560×1440`):** Populated `Analysis` overflows height slightly (~35px, pushing the status footer below the fold) while leaving large empty vertical space below `Opening book` and showing only `Analyze game` (no whole-match trigger).
>   5. **Console & footer polish:** `Settings` interaction emitted `ResizeObserver loop completed with undelivered notifications.`, and sparse workspaces (`Playing`, `Study -> Overview`) show tight `"Analyze in Player database"` button proximity above tables or unpinned footer space.

- **ID & Title:** `[ND-UX-01] Playing Coach mode hides Enter-confirm, feedback/cube dialogs lack shortcuts, and global alerts render below Playing`
- **Severity / Effort / Pillar:** `P1` · `S` · `UX/Usability`
- **Evidence:** `PlayWorkspace.tsx:1013-1022, 1991-1999` hides `.play-board-toolbar`'s `Confirm turn` (`shortcut="Enter"`) when `awaitingFeedback` is true. `PlayCoachFeedback.tsx:51-84` renders only plain `<p>` instruction text for checker turns (no button or `Enter` handler unless SVG dice have focus) and omits `shortcut="Enter"` on cube `Continue`. `PlayFeedback.tsx:287-312` (`Retry`, `Solution`, `Continue`/`Anyway`) and `PlayWorkspace.tsx:2273-2301` (`Take`, `Drop`, `Hint`) omit `shortcut` props. `App.tsx:974-1056` renders `<section id="workspace-panel-play">` *above* global `error`/`notice`/`importRun` banners. Three separate inline `EngineLabel` triggers clutter the match header (`PlayWorkspace.tsx:1620-1660`).
- **Impact:** Keyboard-first Play breaks whenever Coach intervenes; Training/Tutor/Cube dialogs require mouse clicks; import/settings errors sit unseen below the board; live header noise competes with the score/board.
- **Recommendation:** Move global banners above `#workspace-panel-play`; render an explicit `Confirm`/`Pass`/`Continue` primary `ActionButton` (`shortcut="Enter"`) inside `PlayCoachFeedback`; add shortcuts (`Enter`, `T`/`P`/`H`/solution) to `PlayFeedback` & `PlayCubeDecision`; collapse secondary `Analysis`/`Rules` metadata into a single tooltip trigger.
- **Verification & Risk Guardrail:** Preserve `PlayBoardDialog` open-focus and `PlayBoardDice` Enter focus; verify `play` E2E specs & `text-navigation.spec.ts`.

---

- **ID & Title:** `[ND-UX-02] Analysis hides whole-match trigger, splits Checker vs Cube navigation, truncates headers, overflows height slightly, and gates 2-col Statistics behind 3000px`
- **Severity / Effort / Pillar:** `P1` · `M` · `UX/Usability | Visual Design`
- **Evidence:** `MatchAnalysisPanel.tsx:195-203, 593-661` triggers only single-game analysis even though backend + `match-analysis-api.ts:6-20` already support whole-match jobs (`gameNo = undefined`; confirmed live at `1800×1169` & `2560×1440` showing only `Analyze game` + `Analysis options`). Live populated `Analysis` also slightly overflows viewport height (~35px, clipping the status footer below the fold) while leaving blank vertical space beneath `Opening book`. `AnalysisWorkspace.tsx:501-506, 868-907, 985-989` only renders `BoardErrorNavigation` inside `Checker` after clicking a cell in `Statistics`, whereas `Cube` sits on an empty `cubeUnselected` state when no cube decision is active. `analysis-match-header.css:57-74` caps `.analysis-match-event` at `300px`. `desktop-analysis.css:614-656` gates the `@container analysis-statistics (min-width: 1050px)` 2-column grid behind `@media (min-width: 3000px)`.
- **Impact:** Users inside Analysis must jump back to `Match database` to analyze the whole match, cannot step through checker errors (`E`/`Shift+E`) directly from `Checker`, lose the status footer below the fold, and view cramped single-column Statistics on `1800×1169` / `2560×1440` displays.
- **Recommendation:** Expose `Analyze match …` inside `Analysis options` / split command button (enabled cleanly after `[ND-FE-06]` portal removal + `[ND-SEAM-09]` cancel route); lock `.analysis-workspace` grid height cleanly to viewport (`100dvh` minus header/footer) so inner panes scroll without pushing the app footer off-screen; remove the `3000px` `@media` gate so Statistics uses 2 columns whenever container width permits (allowing left rail collapse); provide direct Checker `Prev`/`Next` error controls and auto-select the first cube decision/summary in `Cube`.
- **Verification & Risk Guardrail:** Preserve displayed-vs-calculation separation; verify `desktop-analysis.spec.ts` across `1800×1169`, `2560×1440`, `3840×2160`.

---

- **ID & Title:** `[ND-UX-03] Study Flashcards stacks 5 chrome layers before cards, AddCardsPanel duplicates '+' icons, and Reports hardcode 'GNUbg'`
- **Severity / Effort / Pillar:** `P1` · `M` · `UX/Usability | Visual Design`
- **Evidence:** `LearningPanel.tsx:1270-1435` vertically stacks heading/admin actions, `CardSearchControls`, `deckPanel`, `TransferPractice`, and `CardTopics` above the card table. `AddCardsPanel` (`296-354`) places two identical `icon="add"` `compact` buttons (`addPosition` vs `addErrors`) separated by a permanently open `<CreateDeckForm />`. `StudyReports.tsx:245-251` AND backend `study.rs:26-45, 138` hardcode `'GNUbg'` even when the session was played against **NeonGammon**. (Live `Study -> Overview` at `1800×1169` also showed `"Analyze in Player database"` sitting flush against the table header row.)
- **Impact:** Flashcards list sits buried below administrative controls on `1800×1169`; identical `+` buttons force tooltip hunting; money Study reports misattribute NeonGammon matches to GNUbg.
- **Recommendation:** Fold `TransferPractice` and `CardTopics` behind a compact collapsible/segmented strip so search + `Study now` + cards sit above the fold; label `Add position` vs `Add game errors` distinctly and toggle `CreateDeckForm` via `+ New deck`; add breathing room below `"Analyze in Player database"`; extend `StudyReportSummary` (`study.rs`) with `bot_name`/`engine` -> `api:types` -> `StudyReports.tsx`.
- **Verification & Risk Guardrail:** Preserve shared multi-deck card identity & Check-vs-Rate separation; verify `typography.spec.ts` & 6 locales.

---

- **ID & Title:** `[ND-UX-04] Positions breaks navigation context on Similar search and lacks toolbar/keyboard parity across All Analyzed Decisions`
- **Severity / Effort / Pillar:** `P1` · `M` · `UX/Usability`
- **Evidence:** `App.tsx:1197-1212` hides `.position-scope` (`Saved positions` | `All analyzed decisions`) when `SimilarPositions` opens. `DecisionSearch.tsx:142, 264-550` hides its heading (confirmed live at `1800×1169` showing no `<h1>` above the scope tabs), omits `Clear` and `Columns` picker (present in `PositionBrowserResults.tsx:241-269`), leaves initial results blank, and omits `Add to deck` (note: backend `AddPositions` already supports checker `hasPosition` hits; cube hits need batch `MistakeReference` on `AddCards` in Phase C). `SimilarPositions.tsx:192-210` uses `<button>` inside `<td>` instead of `SearchResultRow` keyboard `Up`/`Down`/`Home`/`End`/`Enter`.
- **Impact:** Disorienting context switch when opening Similar positions; missing table customization and keyboard table nav across sibling tabs.
- **Recommendation:** Keep `.position-scope` visible with a `← Saved positions` header bar for Similar; add visible `<h1>`, `Clear`, `Columns`, checker `Add to deck` (plus cube batching in Phase C), and `SearchResultRow` keyboard nav across `DecisionSearch` and `SimilarPositions`.
- **Verification & Risk Guardrail:** `typography.spec.ts`, `text-navigation.spec.ts`, `library-position-results.spec.ts`.

---

- **ID & Title:** `[ND-UX-05]` (P2 · S · UX/Usability | Visual Design) **Match Database crowds 10 low-contrast ungrouped icons, omits `.danger` on delete buttons, leaves Import target unlabeled, truncates columns at `2560×1440`, and spams row tooltips:** `MatchDatabase.tsx:760-858,1276`, `DatabaseSidebar.tsx:99-105`, `LearningPanel.tsx:1150-1167`, `LibraryImport.tsx:32-62`. Confirmed live ([final_step_26.png](file:///Users/hendrik/.gemini/antigravity/brain/12492bdd-facf-41b1-bd2f-f8a618ddf2aa/browser_evidence/92e170a7-f527-464b-b78a-ea032f700d19/final_step_26.png) and `2560×1440`): `Imported on` truncates to `Importe...`, `Play mode` truncates to `Coach · Wi...`/`Competitio...` despite wide blank gaps in `White`/`Black`/`Result`, disabled batch icons have very low contrast on dark themes, and hovering row `ID: 240` pops an opaque overlay across rows `239–236`. **Fix:** Visually group batch actions with better disabled contrast, apply `className="danger"` to delete triggers, rebalance default column widths (`Play mode` & `Imported on` vs `White`/`Black`/`Result`), add visible label before import destination `<select>`, and move `<Tooltip as="tr">` onto a focused info cell/drawer (eliminating 51 tooltip hook mounts/page). **Verify:** `ui-themes.spec.ts`, `text-navigation.spec.ts`.
- **ID & Title:** `[ND-UX-06]` (P1 · S · Visual Design) **Undefined CSS tokens in `play.css`/`game-result.css` and dark-mode contrast failures:** `play.css:271, 559-679` & `game-result.css:5, 19` use undefined `--text`/`--text-muted`/`--border`/`--surface`/`--danger`; `.play-cube-offer strong` (`play.css:707-708`) uses `color: white` on dark pastel `--ui-accent`; resign text (`1066-1105`) hardcodes `#a32929`; `.action-tooltip[popover]` (`action-controls.css:49-52`) hardcodes dark `#172033`. Live `Playing` at `1800×1169` (`Graphite · Dark`) confirmed dim low-contrast `Bot · 0 points` / `167 pips · 0 off` and `You · 0 points` / `167 pips · 0 off` text around the board. **Fix:** Map to `--ui-*` / `--ui-accent-text` / `--ui-danger-text` tokens and raise Play player/pip status contrast in dark palettes. **Verify:** All 6 palettes in `ui-themes.spec.ts`.
- **ID & Title:** `[ND-UX-07]` (P2 · M · Visual Design | Architecture) **Player database & Extras use plain native `<button>` controls:** `PlayerDirectory.tsx`, `PlayerSummary.tsx`, `BenchmarkWorkspace.tsx`, `EngineMatchWorkspace.tsx`, and `DatabaseBackupWorkspace.tsx` mix raw `<button>` elements without `ActionButton` icons, `.primary` hierarchy, or `.text-navigation` mode tabs. **Fix:** Standardize on `ActionButton` + `text-navigation` while checking `DE`/`PL` button wrap at `1800×1169`.
- **ID & Title:** `[ND-UX-08]` (P2 · S · UX/Usability) **`<input type="number">` leaks OS-locale decimals against app-formatted table numbers:** `DecisionSearch`, `RecurringMistakes`, `LearningPanel`, `PlayWorkspace`, `StudyOverview` use native `type="number"` (plus misaligned `0.05` vs `0.08` defaults) instead of `SettingsDialog`'s locale-aware `inputMode="decimal"`. Live `Positions -> All analyzed decisions` (`1800×1169`) rendered **`0,05`** (OS comma) directly above dot-formatted table losses (`1.037`, `1.012`). **Fix:** Extract shared `EquityDecimalInput` bound to active app locale and align default thresholds with profile cutoffs.
- **ID & Title:** `[ND-UX-09]` (P2 · S · UX/Usability | Visual Design) **Header `ProfileSelector` packs inline warnings and lacks one-click `Save and switch`:** `ProfileSelector.tsx:130-148, 213-233` stacks multi-line `<p>` warnings in `.app-header-actions` and omits `save_current` (already supported by backend `POST /api/profiles/{id}/activate`!). **Fix:** Move warnings into a compact header badge popover and expose `Save and switch` directly in the switch confirmation.
- **ID & Title:** `[ND-UX-10]` (P3 · S · Keyboard & Accessibility) **Keycap badges only render in `.study-card`, and splitters reverse double-click behavior:** Promote subtle `[data-shortcut]` keycaps onto primary/review `ActionButton`s in `action-controls.css` (without wrapping `DE`/`PL` at `1800×1169`) and align `useAnalysisBoardResize` vs `useCardPreviewResize` so double-click + `Enter` consistently reset (and `End` maximizes).

---

## 5. Phased Refactoring & Polish Roadmap

### Guardrails: What NOT to Rewrite
- **Keep `worker.py` JSON/stdio IPC** over Rust↔C++ FFI until post-batching profiling shows JSON overhead matters.
- **Keep `job-watch.ts` SSE + `configuration-store.ts` optimistic rebase recovery** (do not replace with Redux/React Query); simply eliminate redundant polls and legacy bridges.
- **Keep `0001_schema.sql` immutable**, explicit SQLx queries (no ORM), and `NumberedSearchPagination` in `DecisionSearch` unless UX explicitly opts into Next/Prev cursors.

### Phase A — Quick Wins (1–3 Days Each, High Immediate Impact & Low Risk)
- **NeonGammon:**
  - Fix `forward_from_base` 64-delta truncation (`[NG-CPP-03]`); replace `MultiPlyStrategy::cache_salt_` raw pointer with `eval_identity()` and fix `SharedPosCache` `STATE_CLAIMED` race (`[NG-CPP-04]`).
  - Add `py::gil_scoped_release` on single-eval, unified cube, and `create_*` bindings (`[NG-PY-01]`); eliminate double 24-NN disk load & double `BearoffStrategy` wrap in `BgBotAnalyzer` (`[NG-PY-02]`).
  - Thread `force_boards`, `beaver`, and `max_cube_value` into `batch_checker_play`, align `post_move_analytics` `cubeful` & `ROOT_JANOWSKI_CUBE_LIFE_PLY`, and fix `jacoby`/`beaver` docs (`[NG-PY-03]`, `[NG-PY-04]`).
  - Prune 6 unused `sl_s9_*.weights.best` files (2.08 MB) + `_tdinit` sdist exclusion (`[NG-PY-07]`); switch `__init__.py` to PEP 562 lazy exports so `text_export` works pure-Python (`[NG-PY-08]`).
- **Cross-Repo Seam & NeonDojo DB/Backend:**
  - Put `analyze_rolls` and chunked `SageEngine::game` inside `VerifiedBatch` + `batch_checker_play`, and add stat-guarded steady-state `verify()` for persistent `Slot::Play` workers (`[ND-SEAM-01]`, `[ND-SEAM-02]`).
  - Fix `play/open_sage.rs` `rating_decisions: 0` (`[ND-BE-01]`); route `similar_positions::score` via `settings.analysis_engine`; expose `bot_name`/`engine` on `StudyReportSummary` (`study.rs` → `StudyReports.tsx`, `[ND-UX-03]`).
  - Split colliding `91432302` advisory lock (`[ND-SEAM-08]`), batch `copy_children` + add `matches_played_record_idx` (`[ND-SEAM-05]`), disable `0086`/`0089`/`0090` triggers during backup `COPY` (`[ND-SEAM-07]`), raise `PgPool::max_connections` + add `/api/jobs/{id}/cancel` (`[ND-SEAM-09]`), offload SGF/fs blocking onto `spawn_blocking` (`[ND-SEAM-10]`), validate `play.engine` changes + share `OPTIONS` (`[ND-BE-05]`), and adopt shared `ApiJson`/`ApiQuery` extractors + fix runtime `unwrap()`s (`[ND-BE-03]`, `[ND-BE-09]`).
- **NeonDojo Frontend & UX/UI:**
  - Fix `play.css` / `game-result.css` `--ui-*` tokens and dark-mode contrast (`[ND-UX-06]`); move global alerts above Playing (`[ND-UX-01]`); add Coach `Enter` confirm & Feedback/Cube shortcuts (`[ND-UX-01]`).
  - Remove `3000px` `@media` gate on 2-col Statistics (`[ND-UX-02]`); group Match DB batch bar, add `.danger`, remove `<Tooltip as="tr">` (`[ND-UX-05]`); add `Save and switch` + popover warnings in `ProfileSelector` (`[ND-UX-09]`); align splitter double-click semantics (`[ND-UX-10]`).
  - Cache `Intl` formatters (`[ND-FE-04]`), stop idle 1s `BenchmarkWorkspace` polling (`[ND-FE-07]`), lazy-mount `SettingsDialog` (`[ND-FE-05]`), and fix render-time `setState`/ref mutations & raw `fetch()` (`[ND-FE-09]`, `[ND-FE-10]`).

### Phase B — Modularity & Seam Refactors (1–2 Weeks)
- **NeonGammon Performance & Clean Public Seam:**
  - Implement `BearoffStrategy::batch_evaluate_*`, batched `best_move_index_cubeful_multi`, unified C++ 1-ply candidate batching (obeying `SERIAL_CUBEFUL_1PLY`), `sigmoid_inplace_neon` arm64 alignment, and `eval_groups_pair` bucketing (`[NG-CPP-01]`, `[NG-CPP-02]`, `[NG-CPP-03]`).
  - Cap `MultiPlyThreadPool` to M4 Pro P-cores (`10`) + atomic work stealing + `thread_local` scratch (`[NG-CPP-05]`); unify Rust `CPU_BUDGET` across GNUbg & NeonGammon + isolate benchmarks from Play (`[ND-SEAM-03]`).
  - Export `Policy` `Protocol`, public `BgBotAnalyzer` `top_two`/perspective methods, and `DifficultyPolicy(analyzer=..., eval_level=..., reseed)`, eliminating all private `_` hooks in `worker.py` and aligning 3-ply `FullStrength` (`[NG-PY-05]`, `[ND-SEAM-04]`).
  - Add in-memory MLX epoch scoring (`[NG-PY-06]`), `py.typed` + `TypedDict`s (`[NG-PY-10]`), and missing pure-Python unit tests (`[NG-PY-12]`).
- **NeonDojo Architecture, SQL & Frontend Restructuring:**
  - Add coarse profile columns/indexes for `similar_positions` SQL pre-filtering + `position_browser` / `jobs` / `decision_index` indexes (`[ND-SEAM-06]`), and cooperative restore drain (`[ND-SEAM-07]`).
  - Define `EngineProvider` capability trait (`[ND-BE-01]`), split Rust mega-modules into `routes`/`service`/`repo` (`[ND-BE-02]`), and unify `ErrorBody` + retire `ConfigurationApiError`/`SettingsApiError`/`configuration-settings` bridge (`[ND-BE-03]`, `[ND-FE-05]`, `[ND-FE-09]`).
  - Feature-slice `web/src/` (`[ND-FE-01]`): decompose `PlayWorkspace` (`usePlayAnimation` isolation, `PlayHistory` memo), `LearningPanel`, `SettingsDialog`, and `AnalysisWorkspace` (delete 4 portals + `MutationObserver`, expose `Analyze match …`, drive catalog/DB updates from SSE `[ND-FE-06]`, `[ND-FE-07]`).
  - Code-split idle workspaces, per-locale i18n bundles, and `generated-themes.json` (`[ND-FE-02]`), update `useAnalysisBoardResize` / `useResizableColumns` via CSS variables (`[ND-FE-04]`), and adopt `ActionButton` across `Players`/`Extras` (`[ND-UX-07]`).

### Phase C — Strategic Extensibility & UX Simplification
- **NeonGammon:** Extract pure-C++ `bgbot::AnalyzerSession` shared by `mobile.cpp` & `bindings.cpp` (`[NG-CPP-09]`), split `bindings.cpp`/`rollout.cpp`/`analyzer.py` (`[NG-CPP-07]`, `[NG-PY-02]`), unify `cube_eval` single-pass root `probs` (`[NG-CPP-08]`), consolidate `xg_compare.py` onto `xg_file.py` (`[NG-PY-09]`), curate `scripts/` (`[NG-PY-11]`), and introduce manifest-driven `RoutedStrategy` for Stage 12+ (`[NG-CPP-10]`).
- **NeonDojo:** Extract pure `backgammon/` domain IDs & checker rules (`[ND-BE-04]`), `import::FormatRegistry` (`[ND-BE-06]`), typed `ExerciseKind` & isolated `learning/anki/*` (`[ND-BE-08]`), move stateful statics to `AppState` & extract `neondojo::runtime::start` for Tauri (`[ND-BE-07]`, `[ND-BE-09]`); unify `Positions`/`Decisions`/`Similar` shared table/filter UX + `EquityDecimalInput` (`[ND-UX-04]`, `[ND-UX-08]`), declutter Study Flashcards (`[ND-UX-03]`), unify i18n onto `formatMessage` (`[ND-FE-08]`), invert `main.tsx` CSS import order & prune dead `style.css` (`[ND-FE-03]`), and promote keycap shortcuts (`[ND-UX-10]`).

---

## 6. Verification & Non-Regression Plan

Every phase must gate on the documented verification suites without touching the user's live PostgreSQL library (`neondojo`):

1. **NeonGammon C++ & Python Gates:**
   - Rebuild Accelerate (`build/cmake_accel`) and non-Accelerate (`build/cmake_portable`, `-DBGBOT_USE_ACCELERATE=OFF`) extensions (`rm -f build/bgbot_cpp*.so` before `cp`) and `mobile/CMakeLists.txt`.
   - Core regression & parity checks:
     ```sh
     PYTHONPATH="$PWD/build:$PWD/python" python -m pytest -q \
       tests/test_encoding_golden.py \
       tests/test_sl_train.py \
       tests/test_batch_checker_play_parity.py \
       tests/test_2ply_details.py \
       tests/test_vr_and_dead_cube.py \
       tests/test_contact_cube_life.py \
       tests/test_cubeful.py \
       tests/test_simulation.py \
       tests/test_puretd.py \
       tests/test_difficulty.py
     ```
   - Pure-Python import check with `bgbot_cpp` hidden, `mypy` / `pyright` typecheck, `mobile/test_parity.py`, wheel build smoke test, and before/after Apple Silicon M4 Pro timings (`profile_nn_inference`, `benchmark_3t`, `benchmark_4ply`).
2. **NeonDojo Backend, SQL & Engine Seam Gates:**
   - Export maintenance DB before running tests (`DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres`):
     ```sh
     cargo fmt --all --check
     cargo clippy --workspace --all-targets -- -D warnings
     DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres cargo test --workspace
     ```
   - Native engine & NeonGammon integration tests (`-- --ignored --test-threads=1` for `open_sage_*` / `play` / `settings` / `match_analysis`), plus `npm run test:themes --prefix web` (runs `tools/test_*.py` including `probe_open_sage.py`).
   - Run `EXPLAIN (ANALYZE, BUFFERS)` in a seeded isolated test DB (`10k+` matches, `500k+` positions) on `similar_positions`, `duplicate_matches`, `position_browser`, and `recurring_mistakes/search`, plus restore benchmark verifying zero restore `pg_notify` emissions and passing `database_backup` fingerprint checks.
3. **NeonDojo OpenAPI, Frontend & Visual UX Gates:**
   - Regenerate and typecheck contracts after any Rust API/DTO change:
     ```sh
     npm run api:types --prefix web
     npm test --prefix web
     npm run typecheck:e2e --prefix web
     npm run build --prefix web
     ```
   - Run Playwright browser checks across all 6 UI locales (`EN`, `DE`, `ES`, `FR`, `IT`, `PL`), all 6 light/dark palette themes (`graphite`, `club`, `paper`), and desktop viewports (`1800×1169`, `2560×1440`, `3840×2160`), inspecting live screenshots visually:
     ```sh
     npm run test:e2e --prefix web -- e2e/text-navigation.spec.ts e2e/typography.spec.ts e2e/ui-themes.spec.ts e2e/dice-equity.spec.ts e2e/settings.spec.ts
     ```
   - After final edits, verify live startup and health on port `8787` via `./tools/build_local_restart.sh` + `curl --fail --silent --show-error http://127.0.0.1:8787/api/health`.

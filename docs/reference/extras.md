# Extras

How version information, engine benchmarks, engine duels and database backups behave. For a guided introduction, see the [user guide](../user-guide/README.md#8-extras).

## Version information

Open **Extras → Versions** for the running NeonDojo build and the availability and versions of GNU Backgammon and NeonGammon. **Technical details** shows the complete build commit and engine version output, including the source commit link for recognized NeonGammon forks. **Copy version information** copies the full report for troubleshooting. If one request fails, **Try again** retries that request and keeps the information already loaded.

`GET /api/version` returns the full commit ID in `version`, or an empty string when it is unavailable. `GET /api/engines` includes the repository and full source commit in the optional `fork` field.

## Engine benchmark

In **Extras → Benchmark**, choose an engine for both **Match analysis** and **Play-turn latency**. Each measurement type has its own history and comparison. GNUbg supports 1–5 ply; NeonGammon supports 1–4 ply after a capability probe. Depths initially come from Settings. The benchmark remembers depth selections separately for each engine while the app is open and flags an automatic fallback to a supported depth. Selected analysis depths run in increasing order in the background and continue when you leave the view. **Cancel run** also stops the active engine process. Other NeonDojo engine work waits while a benchmark is in progress, and a notice appears across workspaces. Other programs on the machine can still affect the measurement.

The fixed **WC2014 v1** reference is the 11-point match between Akiko Yazawa and Douglas Mayfield, with ten games and 460 checker moves. The app bundles an immutable copy of the unanalysed GNUbg SGF fixture, with a reference ID, version, and SHA-256 checksum. See the [fixture documentation](../../backend/tests/fixtures/README.md) for provenance and reproduction. Existing evaluations are cleared before analysis.

**Total analysis time** sums the full analysis pipeline for each game, including process startup, loading, checker and cube analysis, and result preparation. Each game starts a fresh GNUbg process. The analysis is cubeful and deterministic, with zero noise and luck analysis disabled. The timer excludes waiting, installation and version checks, and database writes. **Ø analysis time / move** divides the total by all 460 checker moves, including forced moves. It represents the match-wide average. New GNUbg runs use `gnubg-match-v3-2ply-top14` at 3-ply, `gnubg-match-v3-4ply-top14` at 5-ply and `gnubg-match-v2-unlimited` at other depths. GNUbg protocol ids name GNUbg's native level ([ply convention](settings.md#ply-convention)). These protocols have no fixed per-game timeout. Older runs retain their recorded filter and timeout.

**Measure play-turn latency** queues a separate run using the benchmark engine and play-depth selectors (GNUbg 1–3 ply; NeonGammon 1/2-ply). The `gnubg-checker-turn-v1` protocol plays 11 fixed checker turns with dice already rolled, sampled from opening, contact, race and bear-off positions in the WC2014 reference. Each sample restores its own SGF checkpoint in a fresh process through the Playing adapter, plays one bot turn, and reads the resulting snapshot. The timer includes preparation, native process startup, checkpoint restoration, version guards, and result processing. Queue wait, installation checks, database writes, network time and animation are outside the timed scope. The history reports the arithmetic mean in milliseconds. Play measurements use the same queue and cancellation controls. They are independent of analysis-time estimates and do not create games in the library.

History shows date, engine, depth, timing, and status. Expand **Details** for hardware, the complete engine version and executable path, the reference, and the protocol. History is independent of library matches and profiles. To remove a finished run (completed, failed, or cancelled), select the trash icon in its row and confirm with **Delete**; **Keep** or Escape leaves it in place. Deletion is permanent and removes the run from the comparison. A deleted completed analysis run no longer calibrates [analysis time estimates](analysis.md#analysis-time-estimates); a running analysis keeps the average it started with. Queued and running runs have no delete action; cancel them under **Current runs** first (`DELETE /api/benchmarks/{id}` returns 409 for them). Backend restarts mark interrupted runs as failed. A new run starts its timing from the beginning. During a run, the engine availability indicator reuses the verified installation snapshot to avoid starting a competing probe.

| Method | Endpoint                      | Purpose                                                                             |
| ------ | ----------------------------- | ----------------------------------------------------------------------------------- |
| GET    | `/api/benchmarks/reference`   | Read the versioned reference and measurement protocol.                              |
| GET    | `/api/benchmarks/status`      | Read the active status shown in the global notice.                                  |
| GET    | `/api/benchmarks`             | List the latest 1,000 attempts, newest first.                                       |
| POST   | `/api/benchmarks`             | Queue attempts with a request such as `{ "engine_id": "gnubg", "plies": [1, 3] }`.  |
| POST   | `/api/benchmarks/play`        | Queue a play-turn run with `{ "engine_id": "bgsage", "plies": 2 }`; omitted body preserves the legacy saved-play-default behavior. |
| POST   | `/api/benchmarks/{id}/cancel` | Cancel a queued or running attempt. Finished results are retained.                  |

NeonGammon uses depth-specific `bgsage-match-v3-{depth}ply-4threads` and `bgsage-checker-turn-v3-{depth}ply-2threads-gnubg-rules` protocols. A fresh sidecar starts once per run and is then reused. Total elapsed time includes that one initialization plus the measured application pipeline; initialization is also shown separately. Each analysis game is an atomic block with complete installation verification before and after it; no result is returned or persisted before the final check succeeds. Individual requests retain their own boundary checks. Hashes are reused only within one check, never across checks. Playing retains one GNUbg rule process from checkpoint inspection through action execution, then terminates it before committing the result. Playing timing includes preparation, SGF restoration and action execution. Historical v2 results remain available, but are not pooled with v3 timings. Once three complete runs share the same machine, provider, full installation/model/rules snapshot, profile and workload, the view shows the median and minimum–maximum range of the three latest runs. The comparison shows the recorded thread count when known. Expand a group to see the engine version, the measurement protocol, and the GNUbg rules version for NeonGammon when recorded. Times measure application execution, not engine playing strength. ETA calibration never borrows GNUbg timings for Sage.

The server creates all attempts for a multi-depth request together before starting them. New requests during a benchmark return HTTP 409. Unavailable engines and invalid depths return HTTP 422. OpenAPI documents the endpoints and types.

## Engine duels

Open **Extras → Engine duels** to see the selected series and its results. Select **New series** and choose a provider and playing strength for each side, the format, a match length from 1 to 25 points, and up to 1,000 matches or pairs. GNUbg executes legal checker and cube actions and applies scoring and the Crawford rule. Each engine makes its own decisions. Both engines offer 1- to 4-ply for duels, GNUbg also 5-ply. The same number means the same search depth for both engines ([ply convention](settings.md#ply-convention)). NeonGammon requires a local installation that passes a real probe at the requested depth.

The default **Paired with sides swapped** plays two complete matches per seed. The opening dice and the following rolls stay with their board sides when the engines swap. Each game number uses its own sequence, so a game ending early does not shift the dice for the next game. **Independent random rolls** uses a fresh sequence for each match. A blank seed under **Advanced** generates a fresh series seed. **Repeat series …** copies the configuration into the form so it can be run again. Exact replay also depends on the same engine installations and the dice protocol recorded with the original run.

Series keep running in the background. Each completed match is automatically saved under **Match database → Engine Duels** as the series advances. Its **Event** records both engines and strengths in series order, for example **Duel GNUbg 3-ply vs. NeonGammon 3-ply**. The same event applies to both legs of a pair. Replaying a seed creates new match entries. Existing series results stay in the series archive. Cancellation preserves completed results. If a worker loses its lease, the next worker restarts the unfinished match with the same seed. Download individual matches as SGF, or select **Analysis** in a result row to open the match in the Analysis workspace. Analysis opens the library entry with the identical SGF, also after it was moved to another database. If the entry was deleted from the library, or the match was played before automatic saving, NeonDojo saves it again under **Engine Duels** first (`POST /api/engine-matches/{id}/matches/{index}/library`). In paired mode the win rates count only complete pairs. From 20 complete pairs, a 95% confidence interval for A's match win rate is shown. Each pair mean (0, ½ or 1) counts as one observation, and the interval uses the Student t distribution with the sample variance of the pair means, clamped to 0–100%. That variance includes the variance reduction from swapping sides. If every pair has the same mean, the interval has zero width. A first leg without its return leg remains visible in the results.

**CPU time per move** shows each side's measured engine work, divided by that side's checker moves including forced moves and passes. CPU time adds up the work of all engine threads, so engines with different thread counts stay comparable. **Elapsed time** is the wall-clock time of the same decisions. Both include cube decisions. Process start, SGF replay, version checks and NeonGammon integrity checks are excluded: GNUbg is timed around each native bot turn, and NeonGammon around each evaluation in its worker. Matches played before this measurement have no cost data and are left out. The line **Matches with measurement** then shows how many counted matches are included. CPU time is the primary measure: GNUbg's elapsed time also contains roughly 12 ms of idle time inside each native bot turn, independent of depth. The values depend on the computer and its load. [CPU budget concept](../engine-duel-cpu-budget.md)

**Format → Money games without cube** plays blocks of independent money games instead of point matches. The cube is disabled, gammons and backgammons count, and the Jacoby rule does not apply. GNUbg plays with cubeless evaluation; NeonGammon uses its cubeless checker path, which evaluates serially and searches only its filtered top candidates at the full depth. **Games per block** (1–1,000, default 100) sets how many games one stored unit holds, and the count sets the blocks or block pairs, up to 1,000. For example, 500 block pairs of 100 games play 100,000 games. Each block is one native money session, archived as one entry under **Match database → Engine Duels** with the event suffix **(money, cubeless)**, so large series do not flood the library. Pairing works per block: both legs replay the same dice for each game number with the sides swapped. The result shows the game win rate, **Points per game for A** (−3 to 3) and the share of gammons and backgammons won. From 20 block pairs, or 20 blocks with independent rolls, a 95% interval for the points per game uses the Student t distribution over the per-pair or per-block means; choose the block size so that a series has at least 20 of them. Every native step reloads the session played so far, which takes about 0.1 s per 100 games against NeonGammon, so very large blocks slow down mixed duels.

New duels retain NeonGammon's checker evaluations from play, including up to five candidates, probabilities, depth and engine/model provenance. These appear alongside GNUbg's saved evaluations in Analysis and survive NeonDojo's SGF export/import. The NeonGammon values use the custom `BGTSC` SGF property; other SGF applications may ignore it. These are each engine's own playing evaluations, not a full match analysis by a common engine. Previously completed duels without saved Sage evaluations remain unchanged; select the duel in the **Match database** and use **Analyze selected matches …** to obtain a new analysis of both players.

## Database backup

On the source installation, open **Extras → Database backup → Export backup**.
The downloaded `.neondojo-backup` file contains the PostgreSQL application data,
including all match databases, original match files, notes, completed analyses,
saved positions, player profiles, preferences and learning history.

On the destination, open **Extras → Database backup** and choose a file. NeonDojo
checks the archive and compares its migration fingerprint with the destination
database. The preview shows when the archive was created and how many library
matches, saved positions, and flashcards it contains. If an older archive lacks
a recorded creation time, NeonDojo uses a valid original filename
(`neondojo-YYYY-MM-DD-HHMMSS.neondojo-backup`, UTC) and labels the time
**from filename**. A successful check enables **Review database replacement**.
The confirmation dialog shows the creation time and compares the backup counts
with the destination counts fetched when the dialog opens.
**Difference** shows how many entries each category gains or loses after
replacement. Increases have a plus sign and appear in green. Decreases have a
minus sign and appear in red. Unchanged counts show a neutral zero.
The check uses a temporary upload and leaves the destination data intact.
Damaged archives and incompatible schemas prevent confirmation.

**Replace database** overwrites the application data on the destination. Export
its current database first if you need to keep that data. After confirmation,
the backend checks the archive and schema again. PostgreSQL validates the data
as the replacement is applied in one transaction. If restore fails, the
previous data stays in place. Wait for the success message and reload NeonDojo.
Finish any work running on the destination before restoring.

The destination keeps its local engine installations, thread overrides, and
opening-book paths. In-flight or queued jobs from the backup are marked as
interrupted. Saved playing checkpoints still require compatible local engine
versions or models. Install external engines and opening books on the destination
through their normal setup. The archive holds unencrypted data. It supports up to
16 GiB compressed and 64 GiB uncompressed. Large restores need temporary disk
space for both forms of the archive.

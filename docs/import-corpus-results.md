# Import check of the local file corpus

This page collects the dated results of the optional corpus test `all_local_game_files_import_atomically_and_deduplicate` ([`backend/tests/import_corpus.rs`](../backend/tests/import_corpus.rs)). Each run imports all files from `games/xg`, `games/mat`, and `games/bgf` through the actual multipart REST handler into a separate SQLx test database. The personal match database is not used for this. The test requires every supplied file to be imported.

Latest run: November 20, 2026. The two older sections from November 8, 2026 describe earlier corpora and import rules and are kept as history. [Rerun](#rerun) describes the test command.

## November 20, 2026 check: tournament transcripts

The nine tournament files rejected on November 8 are now imported. The corpus test ran with 41 local XG files from the corpus on Halle4 (tournament transcripts, league matches, and four sessions of 200 games each) through the multipart REST handler in an isolated SQLx test database: **41 files, 41 imports, 0 errors.** The import explicitly resolves the following irregularities and reports them with the result:

- `InvalidM=2`: Nine played moves in eight files (R1-04, R1-05, R1-11, R1-14 twice, R2-01, R2-02, R2-05, and Michy-Junko Nakamura). The import continues from the position stored by XG and does not store the move as a decision.
- `R2-04_Statter-MullerKreth.xg`: The open sixth game remains without a result. The seventh game starts with the edited first position.
- `R2-06_Birkle-Larsen.xg`: The first game starts with the stored position instead of the standard starting position.

Every other stored position must match the replayed game. The four large sessions with about 30,000 checker records pass this check.

## Recheck after the private corpus changed

As of November 8, 2026 (historical). During that day the local corpus changed: 20 duplicate paths were removed and 25 files were added. The current full run covers **42 files: 33 imported successfully, nine XG files strictly rejected** (47,936 stored positions in the isolated test data). All MAT and BGF files succeed; the nine rejections concern newly added tournament files.

- Seven files contain `InvalidM=2` in checker records that were actually played. The [official XG format description](https://www.extremegammon.com/xgformat.aspx) defines this marker as invalid. Affected: R1-04, R1-05, R1-11, R1-14, R2-01, R2-02, and R2-05.
- `R2-04_Statter-MullerKreth.xg`: seven game headers and six game endings; after the still open sixth game, a seventh begins.
- `R2-06_Birkle-Larsen.xg`: the stored first checker position contradicts the declared starting position; this leads to an impossible bar entry on the next move.

Archives, stream CRC, and record version 30 were decoded and checked independently. The existing twelve XG regression tests pass. These files are not stored as complete matches by weakening the import validation; the personal match database and the source files remained untouched during the diagnosis. The optional test `all_local_game_files_import_atomically_and_deduplicate` stayed red for this corpus and named all rejections; for the current state, see the November 20 check. The earlier 37/37 report below describes the corpus at that time.

### Earlier corpus of November 8, 2026

**37/37 successful, 17 distinct contents, 20 detected duplicates, and 14,836 stored positions without duplicate copies.** HTTP 201 denotes a new import; HTTP 200 denotes an identical file that was already imported. The checks compare the stored position count and the identity of duplicates; a failed import must not leave a partial game behind.

| Format | Files | Distinct contents | Successful |
| --- | ---: | ---: | ---: |
| XG | 27 | 7 | 27 |
| MAT | 5 | 5 | 5 |
| BGBlitz BGF | 5 | 5 | 5 |

Fixed: valid XG interim states without a match conclusion, MAT resignation lines and combined opening rolls, and the incorrect requirement to use further dice after the last checker was borne off. BGF 1.0 was added as a native Rust import, including Smile/Gzip, forced passes, and cube decisions. Unknown variants, invalid played moves, and corrupted transports are still explicitly rejected.

| File | HTTP | Positions | Duplicate |
| --- | ---: | ---: | --- |
| `bgf/test1.bgf` | 201 | 527 | no |
| `bgf/test2.bgf` | 201 | 579 | no |
| `bgf/test3.bgf` | 201 | 357 | no |
| `bgf/test4.bgf` | 201 | 460 | no |
| `bgf/test5.bgf` | 201 | 460 | no |
| `mat/test1.mat` | 201 | 846 | no |
| `mat/test2.mat` | 201 | 912 | no |
| `mat/test3.mat` | 201 | 836 | no |
| `mat/test4.mat` | 201 | 554 | no |
| `mat/test5.mat` | 201 | 486 | no |
| `xg/1 (1).xg` | 201 | 7476 | no |
| `xg/1 (10).xg` | 200 | 7476 | yes |
| `xg/1 (11).xg` | 200 | 7476 | yes |
| `xg/1 (12).xg` | 200 | 7476 | yes |
| `xg/1 (13).xg` | 200 | 7476 | yes |
| `xg/1 (14).xg` | 200 | 7476 | yes |
| `xg/1 (15).xg` | 200 | 7476 | yes |
| `xg/1 (16).xg` | 200 | 7476 | yes |
| `xg/1 (17).xg` | 200 | 7476 | yes |
| `xg/1 (18).xg` | 200 | 7476 | yes |
| `xg/1 (19).xg` | 200 | 7476 | yes |
| `xg/1 (2).xg` | 200 | 7476 | yes |
| `xg/1 (20).xg` | 200 | 7476 | yes |
| `xg/1 (3).xg` | 200 | 7476 | yes |
| `xg/1 (4).xg` | 200 | 7476 | yes |
| `xg/1 (5).xg` | 200 | 7476 | yes |
| `xg/1 (6).xg` | 200 | 7476 | yes |
| `xg/1 (7).xg` | 200 | 7476 | yes |
| `xg/1 (8).xg` | 200 | 7476 | yes |
| `xg/1 (9).xg` | 200 | 7476 | yes |
| `xg/1.xg` | 200 | 7476 | yes |
| `xg/Hendrik-Beginner 21 point match 25-10-2026.xg` | 201 | 160 | no |
| `xg/Hendrik-Intermediate 21 point match 24-10-2026 #2.xg` | 201 | 230 | no |
| `xg/Hendrik-Intermediate 21 point match 24-10-2026.xg` | 201 | 279 | no |
| `xg/Hendrik-Intermediate 7 point match 26-10-2026 #2.xg` | 201 | 225 | no |
| `xg/Hendrik-Intermediate 7 point match 26-10-2026.xg` | 201 | 227 | no |
| `xg/Hendrik-XG Roller+ 5 point match 24-10-2026.xg` | 201 | 222 | no |

## Rerun

```sh
DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres \
NEONDOJO_CORPUS_REPORT=/tmp/neondojo-import-results.json \
  cargo test -p neondojo --test integration import_corpus:: -- --ignored --nocapture
```

`DATABASE_URL` points to a maintenance database on a PostgreSQL server with a `CREATEDB` role; SQLx creates its own test database there and removes it again (see [PostgreSQL and GNUbg integration tests](development/testing.md#postgresql-and-gnubg-integration-tests)). The test requires GNUbg; if it is not on the `PATH`, set `GNUBG_BIN`. `NEONDOJO_CORPUS_REPORT` writes the per-file report as JSON. `NEONDOJO_GAMES_DIR` can specify a different root folder with the three format subfolders. The local game files remain unversioned. If one of the subfolders `xg`, `mat`, or `bgf` is missing, the optional corpus test skips its run; the portable format and rule tests run independently of it.

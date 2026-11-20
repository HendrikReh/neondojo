# Import fixtures

`WC2014.xg` and `WC2014.mat` are the publicly downloadable 11-point tie-breaker
between Akiko Yazawa and Douglas Mayfield in the 2014 World Championship final.
Downloaded on 2026-11-08 from the publisher's analyzed matches page:

- https://www.extremegammon.com/V2/WC2014/game0.htm
- https://www.extremegammon.com/V2/WC2014/WC2014.xg
- https://www.extremegammon.com/V2/WC2014/WC2014.mat

`WC2014.gnubg.sgf` is the companion MAT imported and saved by the locally
installed GNU Backgammon 1.05.002 (no analysis). It is a real independent oracle
for the binary importer's 460 checker-play decisions across 10 games. The MAT
export swaps the XG file's player colors, so the tests compare on-roll position
IDs and names and swap the color-dependent score/cube fields when comparing
match IDs. A few checker submoves are ordered differently but reach the same
positions. This fixture is test data, not an evaluation or model substitute.

Reproduction commands inside a GNUbg command file:

```
set confirm default yes
set automatic game off
set automatic roll off
import mat WC2014.mat
save match WC2014.gnubg.sgf
quit
```

Run with `gnubg -t -q -r -c commands.txt` in the fixture directory.

Format references consulted:

- https://www.extremegammon.com/xgformat.aspx
- https://www.extremegammon.com/xg_format.pas
- https://www.extremegammon.com/ZLIBArchive.pas
- https://www.red-bean.com/sgf/backgammon.html
- https://www.gnu.org/software/gnubg/manual/gnubg.html

The official XG example uses save version 31, RichGame version 1, 2560-byte
TSaveRec records, and a compressed 532-byte-per-entry ZLIBArchive registry.
Observed distinctions from the old prose documentation: checker Moves[] are
zero-based source/destination pairs (negative destinations bear off); CubeA
ownership is absolute player 1/player 2; CubeB in cube records uses the same
encoding. PositionI uses NeonDojo's absolute board (player 1 positive, index 25
player 1's bar, index 0 player 2's bar); PositionEnd is from the mover's
perspective. The importer replays explicit checker moves and cube decisions and
requires every PositionI to equal the replayed board. It takes a recorded
position only as a game's start (from the first checker record and the first
cube state) and after a move with InvalidM 2, which XG keeps although it is
illegal.

Support boundaries: standard backgammon, XG2 save versions 24–40, match play
and money sessions without automatic doubles. Edits after a game's first
checker record, beavers/raccoons, nonstandard variants, InvalidM codes other
than 0 and 2, and unsupported versions are rejected explicitly. XG native candidate and cube evaluations, original XG depth labels, and
rollout references are retained separately from new GNUbg jobs. Original
artifacts retain complete rollout streams and comments byte for byte; RTF
comments are not interpreted. Bounded zlib/base64 SGF extensions transport
foreign analysis without inflating long sessions beyond the SGF size limit. SGF supports absolute setup coordinates and repeated stones,
cube setup (standard CO and GNUbg CP), scores, Crawford and Jacoby. Ambiguous
SGF variation trees are rejected; export the played main line first.


## Native game-analysis oracle

`analysis-game1.sgf` and `analysis-game1.txt` were generated from game 1 of
`WC2014.gnubg.sgf` with GNU Backgammon 1.05.002 on 2026-11-08. They are native
0-ply game-analysis output, including all exported checker candidates, cube
analysis and an explicit forced dance. The text export uses normalized equity
at six decimal places. Native SGF `DA` values in match play contain MWC and
must not be interpreted as normalized cube equity.

Reproduce with `gnubg -t -q -r -c commands.txt`, using:

```
set confirm default yes
set confirm new no
set display off
set analysis chequerplay evaluation plies 0
set analysis cubedecision evaluation plies 0
set analysis luck off
set output mwc off
set output digits 6
set export moves number 10000
set export cube display unmarked on
load match WC2014.gnubg.sgf
first game
analyse clear game
analyse game
save game analysis-game1.sgf
export game text analysis-game1.txt
```

The engine integration tests also analyze all ten games (460 checker moves),
check the nonzero-score wrong-pass decision in game 2, and exercise a played
move below the first five candidates at 0, 1 and 2 plies. They require a real
GNUbg installation and can be run with:

```
GNUBG_BIN=/path/to/gnubg cargo test -p neondojo match_analysis::tests --lib -- --ignored
```

## Additional XG import regressions

Portable tests derive version-26 money-session and compact-move cases from the
public WC2014 fixture and rewrap them with valid compression and checksums. They
cover inactive cube placeholders (`Actif=0`, `double=-2`), money sessions ending
at a completed game without `tsFooterMatch`, and joined checker runs such as
13/7 with 5–1 or a joined portion of a double. The latter must expand to legal
individual dice steps while retaining every original position and match ID.
The normalizer uses the replayed board, enforces bar-entry order and maximum
dice use, and never inserts an unrecorded intermediate hit. Public SGF parsing
still requires explicit individual steps.

Additional rule tests cover interleaved bar entries, both player orientations,
endpoint hits, bearing-off overshoots and the larger-die rule. Records with an
unsupported InvalidM code remain rejected with game, checker-ply and record
coordinates. Derived WC2014 cases cover an XG-accepted illegal move for each
player, a game starting from a recorded board with and without a doubled cube,
an in-progress game followed by the next game, and a PositionI that does not
follow from the replay.

On 2026-11-08, 35 user-provided local XG files (15 distinct contents) were
checked without adding them to the repository. All 14 distinct valid files
retained their complete played-checker counts, including a 200-game,
7,476-position money session. One file carries an explicit invalid-play marker
and remains rejected. The private files are not required by the regression
suite.

On 2026-11-20, 41 user-provided local XG files, including tournament
transcripts, were converted without adding them to the repository. All 41
convert and replay. Eight contain nine XG-accepted illegal moves, two games start
from a recorded position, and one in-progress game is followed by the next
game. The four 200-game sessions (about 30,000 checker records) all pass the
PositionI replay check.

## BGF, MAT and interrupted XG corpus coverage

`tutorial.bgf` is a synthetic, uncompressed JSON BGF 1.0 fixture with three
checker plays, a double/take and a resignation. It exercises REST persistence
and real GNUbg full-game analysis without requiring private input files.
BGF unit tests also wrap synthetic data in JSON/Smile and gzip, test damaged
and oversized transport, reject unsupported rules, and roundtrip all 460
positions and Match IDs from the public native SGF oracle above.

BGBlitz special move codes were checked against the installed producer's
public `bgblitz.Move` predicates: Green player -1, Red +1; special green=7,
red=1 double,2 take,3 drop,4 win,5 dance,6 resignation proposal,8 dummy,9
accepted resignation. Dance dice occupy from[0]/to[0]. The converter preserves foreign analysis fields independently and reconstructs
legal explicit moves by replay. BGBlitz per-candidate ply and the explicit
match-equity flag are retained; absent engine provenance remains unknown.
Format/library references: https://www.bgblitz.com/ and
https://docs.rs/serde-smile/0.3.1/serde_smile/ .

New MAT regressions cover a numbered `wants to resign` cell followed by an
opposite-player `Wins` result, native joined overshoot bearoffs, both colors,
and the rule that bearing off the final checker ends the game immediately.
Malformed resignation cells and truncated/illegal tails remain rejected.

Interrupted point-match XG tests retain all played prefix positions, nonzero
scores and accepted-double context. A last game is closed only if its native
`InProgress` flag is set. Missing game footers for completed games and missing
match footers after reaching the target still fail.

The optional `import_corpus` SQLx test sends every local file from
`games/{xg,mat,bgf}` through the real REST router in a disposable database;
`mat_corpus` additionally checks the engine normalization boundary. Neither
test requires copying local games into versioned fixtures. See
[the full per-file report](../../../docs/import-corpus-results.md).


## Imported analysis preservation

Synthetic and public-fixture tests cover source engine and native depth labels,
checker candidates, pre-roll and actual cube-event identities, MWC versus
normalized equity, foreign precision, and raw unknown fields. PostgreSQL tests
prove transactional import, independent deep copies, recovery from an older
normalized SGF using exact original XG bytes, and coexistence with later GNUbg
analysis. Source statistics never supply this app's computed PR. The GNUbg
SGF evaluation and rollout encodings follow the producer's `sgf.c`
(`WriteMoveAnalysis`, `WriteDoubleAnalysis`, `WriteRolloutAnalysis`):
https://sources.debian.org/src/gnubg/1.05.002-2/sgf.c/ .


## HedgeHog OGXM

`hedgehog-cdf.ogxm` is the public CdF Super Jackpot match between Raphaël
Ginoulhac and Guilaume Gallé. It was downloaded from the [HedgeHog
archive](https://hedgehog-bg.com/archive) on 2026-11-13. The [OGXM
export](https://hedgehog-bg.com/api/v1/matches/37ae5825-4260-4648-8f9e-967a7f8a5c24/export?format=binary)
is unchanged. `hedgehog-cdf.gnubg.sgf` is the independently saved replay of
the companion [MAT export](https://hedgehog-bg.com/api/v1/matches/37ae5825-4260-4648-8f9e-967a7f8a5c24/export?format=mat),
using GNU Backgammon 1.08.003 and the same commands as the WC2014 fixture.

`hedgehog-cdf-analyzed.ogxm` is the same match downloaded from the [public
analysis view](https://hedgehog-bg.com/analyze/37ae5825-4260-4648-8f9e-967a7f8a5c24)
using its OGXM export on 2026-11-13. It contains real HedgeHog checker and cube
evaluations. Some checker alternatives encode oversized bear-offs with the
distance to the tray. The importer validates these alternatives against the
recorded roll and replayed position.

`hedgehog-analysis.ogxm` is synthetic. Its source is
`hedgehog-analysis.json`; regenerate it with the tagged [HedgeHog reference
codec](https://gitlab.com/eranlambooij/hedgehog-public/-/tags/ogxm-v2.1.0):

```sh
ogxm_convert --to-binary < hedgehog-analysis.json > hedgehog-analysis.ogxm
```

The two files cover real archive replay, position and match IDs, Crawford,
forced passes, cube offer/response, source MWC and depth, and CRC32 rejection.
The binary layout and score rules follow the [OGXM v2
specification](https://gitlab.com/eranlambooij/hedgehog-public/-/raw/main/docs/OGXM_FORMAT_SPEC.md).

`WC1988-final.txt` is the publicly downloadable 41-point final of World Cup I
(Boston, 1988) between Ray Glaeser and Joe Sylvester, kept byte for byte with
its CRLF line endings. Downloaded on 2026-11-20 from
https://www.hardyhuebener.de/downloads/1988world_cup_final_sylvester-glaeser.txt.
It is a MAT transcript with a boxed title instead of `;` metadata lines and
covers `.txt` uploads. Its 45 game-start score rows are the reference for the
imported scores.

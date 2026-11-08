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
ownership is absolute player 1/player 2; cached PositionI/PositionEnd arrays
can retain an earlier orientation. The importer replays explicit checker
moves and cube decisions instead of trusting those cached positions.

Support boundaries: standard backgammon, XG2 save versions 24–40, match play
and money sessions without automatic doubles. Edited/custom XG positions,
beavers/raccoons, nonstandard variants, invalid-marked records and unsupported
versions are rejected explicitly. XG stored analysis/rollouts/comments are
not imported. SGF supports absolute setup coordinates and repeated stones,
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
GNUBG_BIN=/path/to/gnubg cargo test -p bgtutor match_analysis::tests --lib -- --ignored
```

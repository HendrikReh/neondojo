# Importprüfung des lokalen Dateibestands

Stand: 8. November 2026. Alle Dateien aus `games/xg`, `games/mat` und `games/bgf` wurden über den tatsächlichen Multipart-REST-Handler in einer separaten SQLx-Testdatenbank importiert. Die persönliche Bibliothek wurde für den vollständigen Bestandslauf nicht verwendet.

## Prüfung vom 20. November 2026: Turniertranskripte

Die neun am 8. November abgewiesenen Turnierdateien werden jetzt importiert. Der Korpustest lief mit 41 lokalen XG-Dateien aus dem Bestand auf Halle4 (Turniertranskripte, Ligamatches und vier Sessions mit je 200 Partien) über den Multipart-REST-Handler in einer isolierten SQLx-Testdatenbank: **41 Dateien, 41 Importe, 0 Fehler.** Der Import löst die folgenden Unregelmäßigkeiten ausdrücklich auf und meldet sie mit dem Ergebnis:

- `InvalidM=2`: Neun gespielte Züge in acht Dateien (R1-04, R1-05, R1-11, R1-14 zweimal, R2-01, R2-02, R2-05 und Michy-Junko Nakamura). Der Import setzt mit der von XG gespeicherten Stellung fort und speichert den Zug nicht als Entscheidung.
- `R2-04_Statter-MullerKreth.xg`: Das offene sechste Spiel bleibt ohne Ergebnis. Das siebte Spiel beginnt mit der bearbeiteten ersten Stellung.
- `R2-06_Birkle-Larsen.xg`: Das erste Spiel beginnt mit der gespeicherten Stellung statt mit der Grundstellung.

Jede andere gespeicherte Stellung muss der nachgespielten Partie entsprechen. Die vier großen Sessions mit rund 30.000 Checker-Records bestehen diese Prüfung.

## Erneute Prüfung nach Änderung des privaten Korpus

Im selben Arbeitstag wurde der lokale Bestand verändert: 20 Duplikatpfade wurden entfernt und 25 Dateien ergänzt. Der aktuelle vollständige Lauf umfasst **42 Dateien: 33 erfolgreich importiert, neun XG-Dateien strikt abgewiesen** (47,936 gespeicherte Positionen im isolierten Testbestand). Alle MAT- und BGF-Dateien sind erfolgreich; die neun Ablehnungen betreffen neu hinzugekommene Turnierdateien.

- Sieben Dateien enthalten `InvalidM=2` in tatsächlich gespielten Checker-Records. Die [offizielle XG-Formatbeschreibung](https://www.extremegammon.com/xgformat.aspx) definiert diese Markierung als ungültig. Betroffen: R1-04, R1-05, R1-11, R1-14, R2-01, R2-02 und R2-05.
- `R2-04_Statter-MullerKreth.xg`: sieben Spielheader und sechs Spielabschlüsse; nach dem noch offenen sechsten Spiel beginnt ein siebtes.
- `R2-06_Birkle-Larsen.xg`: die gespeicherte erste Checkerstellung widerspricht der deklarierten Startstellung; daraus folgt eine unmögliche Bar-Eingabe beim nächsten Zug.

Archive, Stream-CRC und Recordversion30 wurden unabhängig dekodiert und geprüft. Die bestehenden zwölf XG-Regressionstests sind grün. Diese Dateien werden nicht durch Abschwächen der Importvalidierung als vollständige Matches gespeichert; die persönliche Bibliothek und die Quelldateien blieben bei der Diagnose unangetastet. Der optionale Test `all_local_game_files_import_atomically_and_deduplicate` blieb für diesen Bestand rot und nannte sämtliche Ablehnungen; zum aktuellen Stand siehe die Prüfung vom 20. November. Der nachstehende frühere 37/37-Bericht beschreibt den damaligen Bestand.

**Früherer Bestand: 37/37 erfolgreich, 17 verschiedene Inhalte, 20 erkannte Duplikate und 14.836 gespeicherte Positionen ohne Duplikatkopien.** HTTP 201 bezeichnet einen neuen Import; HTTP 200 bezeichnet eine identische, bereits importierte Datei. Die Prüfungen vergleichen die gespeicherte Positionszahl und die Identität von Duplikaten; ein fehlgeschlagener Import darf keine Teilpartie hinterlassen.

| Format | Dateien | Verschiedene Inhalte | Erfolgreich |
| --- | ---: | ---: | ---: |
| XG | 27 | 7 | 27 |
| MAT | 5 | 5 | 5 |
| BGBlitz BGF | 5 | 5 | 5 |

Behoben wurden gültige XG-Zwischenstände ohne Matchabschluss, MAT-Aufgabezeilen und zusammengesetztes Auswürfeln sowie die falsche Pflicht, nach dem letzten ausgelegten Stein noch weitere Würfel zu verwenden. BGF 1.0 wurde als nativer Rust-Import ergänzt, einschließlich Smile/Gzip, Aussetzen und Dopplerentscheidungen. Unbekannte Varianten, ungültige gespielte Züge und beschädigte Transporte werden weiterhin ausdrücklich abgelehnt.

| Datei | HTTP | Positionen | Duplikat |
| --- | ---: | ---: | --- |
| `bgf/test1.bgf` | 201 | 527 | nein |
| `bgf/test2.bgf` | 201 | 579 | nein |
| `bgf/test3.bgf` | 201 | 357 | nein |
| `bgf/test4.bgf` | 201 | 460 | nein |
| `bgf/test5.bgf` | 201 | 460 | nein |
| `mat/test1.mat` | 201 | 846 | nein |
| `mat/test2.mat` | 201 | 912 | nein |
| `mat/test3.mat` | 201 | 836 | nein |
| `mat/test4.mat` | 201 | 554 | nein |
| `mat/test5.mat` | 201 | 486 | nein |
| `xg/1 (1).xg` | 201 | 7476 | nein |
| `xg/1 (10).xg` | 200 | 7476 | ja |
| `xg/1 (11).xg` | 200 | 7476 | ja |
| `xg/1 (12).xg` | 200 | 7476 | ja |
| `xg/1 (13).xg` | 200 | 7476 | ja |
| `xg/1 (14).xg` | 200 | 7476 | ja |
| `xg/1 (15).xg` | 200 | 7476 | ja |
| `xg/1 (16).xg` | 200 | 7476 | ja |
| `xg/1 (17).xg` | 200 | 7476 | ja |
| `xg/1 (18).xg` | 200 | 7476 | ja |
| `xg/1 (19).xg` | 200 | 7476 | ja |
| `xg/1 (2).xg` | 200 | 7476 | ja |
| `xg/1 (20).xg` | 200 | 7476 | ja |
| `xg/1 (3).xg` | 200 | 7476 | ja |
| `xg/1 (4).xg` | 200 | 7476 | ja |
| `xg/1 (5).xg` | 200 | 7476 | ja |
| `xg/1 (6).xg` | 200 | 7476 | ja |
| `xg/1 (7).xg` | 200 | 7476 | ja |
| `xg/1 (8).xg` | 200 | 7476 | ja |
| `xg/1 (9).xg` | 200 | 7476 | ja |
| `xg/1.xg` | 200 | 7476 | ja |
| `xg/Hendrik-Beginner 21 point match 25-10-2026.xg` | 201 | 160 | nein |
| `xg/Hendrik-Intermediate 21 point match 24-10-2026 #2.xg` | 201 | 230 | nein |
| `xg/Hendrik-Intermediate 21 point match 24-10-2026.xg` | 201 | 279 | nein |
| `xg/Hendrik-Intermediate 7 point match 26-10-2026 #2.xg` | 201 | 225 | nein |
| `xg/Hendrik-Intermediate 7 point match 26-10-2026.xg` | 201 | 227 | nein |
| `xg/Hendrik-XG Roller+ 5 point match 24-10-2026.xg` | 201 | 222 | nein |

## Wiederholen

```sh
DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo NEONDOJO_CORPUS_REPORT=/tmp/neondojo-import-results.json cargo test -p neondojo --test import_corpus -- --ignored --nocapture
```

`NEONDOJO_GAMES_DIR` kann einen anderen Stammordner mit den drei Format-Unterordnern angeben. Die lokalen Spieldateien bleiben unversioniert. Ohne diese Dateien überspringt der optionale Bestandstest seine Ausführung; die portablen Format- und Regeltests laufen unabhängig davon.

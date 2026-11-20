# NeonDojo — erster lauffähiger Stand

Der vom Nutzer vorgegebene Stack ist verbindlich: ein eigenständiger Rust/Tokio/Axum-Prozess, REST und SSE mit OpenAPI, PostgreSQL in Docker über SQLx und das lokal installierte GNUbg. React/TypeScript verwendet ausschließlich die HTTP-Schnittstelle. Tauri bleibt ein späterer Host dieses Clients.

## Architekturentscheidung

Ein modularer Monolith hält Import, persistente Aufträge und Abfragen in einem deploybaren Backend. Gegenüber separaten Services bleibt der lokale Start einfach; gegenüber einem Tauri-internen Backend bleiben CLI, Browser und spätere Desktop-App gleichberechtigte Clients.

## Erster Funktionsumfang

- Originaldateien und normalisierte SGF-Partien speichern; SHA-256 verhindert doppelte Dateiimporte. MAT/SGF über GNUbg und einen SGF-Parser; binäre XG-Dateien über einen begrenzten Decoder der veröffentlichten XG-Spezifikation. Unbekannte Versionen oder beschädigte Daten werden explizit abgelehnt.
- Partien und Zugpositionen transaktional persistieren. Name/Dateiname über PostgreSQL-Trigrammindex, Positionen über GNU-Position-ID und B-tree-Index suchen. Begrenzte Ergebnisse mit stabiler Cursor-Paginierung.
- Ein Auftrag analysiert eine gewählte Zugposition wirklich mit GNUbg. Persistente Zustände queued/running/completed/failed, Timeouts, begrenzte Parallelität, Prozessabbruch beim Shutdown; SSE liefert gespeicherte Ereignisse mit Last-Event-ID.
- Dice Distribution berechnet die besten Züge und Equity für die 21 ungeordneten Würfelpaare (Pasch 1/36, andere 2/36). Partiestatistik zählt tatsächlich geworfene Würfel separat.
- Oberfläche: Import, Partiensuche, Zugnavigation und Brett, exakte Positionssuche, Analyse starten und Ergebnis sehen, Dice Distribution, persistente Board-Themes.

## Grenzen und Fehler

Loopback-Bindung als Standard für eine lokale Anwendung; keine Benutzerverwaltung in diesem Stand. Begrenzte Dateigröße, formatbezogene Fehlermeldungen, kein Shell-Aufruf mit Dateiinhalten. Ein Fehler darf keine halbe Partie speichern. Aufträge werden atomar beansprucht; abgestürzte Aufträge werden nach Lease-Ablauf erneut ausgeführt. Kein fingiertes Analyseergebnis. Vollständige Partieanalyse, Rollouts und Tauri sind Folgeausbau.

## Verifikation

Parser- und Zustandsübergangstests, PostgreSQL-Integrationstests mit echter Migration, echter GNUbg-Auftrag einschließlich Dice Distribution, OpenAPI-Vertragsprüfung, Frontend-Build sowie Browserprüfung des durchgängigen Ablaufs. Binär-XG mit einer echten Referenzdatei prüfen. Suche mit EXPLAIN an einem größeren synthetischen Datenbestand prüfen.

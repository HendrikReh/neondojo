# Würfel-Equity bei Bedarf

Ziel: Die Auswahl von Würfel-Equity startet die vorhandene Dice-Distribution-Berechnung; der separate Startknopf entfällt. Backend, Gewichtung und Jobverträge bleiben unverändert.

- [x] Browsertests für Start durch Tab-Auswahl, Wiederverwendung, Positions-/Tiefenwechsel und Fehlerwiederholung ergänzen und zunächst fehlschlagen sehen.
- [x] `useJobs.ts`: Wiederherstellung gespeicherter Jobs explizit kennzeichnen und bei Positions-/Enginewechsel keine alten Ergebnisse ausgeben.
- [x] `AnalysisPanel.tsx`: Nur bei sichtbarer, ausgewählter Würfel-Equity fehlende Ergebnisse anfordern. Pro unveränderter Auswahl von Position, Engine und Tiefe höchstens ein automatischer Versuch; erneute Auswahl erlaubt einen Wiederholungsversuch. Laufende Jobs abwarten.
- [x] `AnalysisWorkspace.tsx`: Sichtbarkeit und Jobwiederherstellung weitergeben. Sechs Übersetzungen, bestehende Browsertests und Bedienungsdokumentation aktualisieren.
- [x] Frontendtests, Build und gezielte Browserabläufe mit isolierter Testdatenbank ausführen; Desktopansichten bei 1800×1169, 2560×1440 und 3840×2160 prüfen. Diff auf fremde Änderungen prüfen.

Umsetzung im bestehenden Arbeitsverzeichnis, da die betroffenen Komponenten bereits lokale Änderungen enthalten. Commit und Push aller Änderungen nach `origin/main` im abschließenden Folgeauftrag beauftragt.


Verifiziert: 510 Frontendtests und Produktionsbuild erfolgreich; abschließend
8 gezielte Browsertests mit isoliertem PostgreSQL und echtem GNUbg erfolgreich
(einschließlich 4-ply, Fehlern, verzögerten Antworten und schnellem Zurückwechseln).
Desktopansichten bei allen drei Referenzgrößen visuell geprüft. Format- und
Diffprüfung erfolgreich. Unabhängige Prüfung durchgeführt; die bisherige
Speicherung des jeweils letzten Jobs bleibt erhalten, kein neuer Ergebnisverlauf.

Im zusätzlich ausgeführten breiteren Browserlauf bestanden 12 von 13 Tests.
Der bestehende erste Workflowtest erwartet vor der Würfel-Equity-Auswahl noch
den Text „BESTER ZUG“, den die bereits lokal geänderte Kandidatentabelle nicht
mehr rendert. Im Nachfolgeauftrag wurden die Selektoren auf die markierte beste Kandidatenzeile
umgestellt. Der anschließend erreichte Layoutcheck deckte überlaufende
Screenreader-Beschriftungen auf; diese bleiben nun durch lokale Positionierung
im Scrollbereich der Kandidatentabelle. Der vollständige betroffene Workflowtest
besteht jetzt, ebenso 512 Frontendtests und der Produktionsbuild.

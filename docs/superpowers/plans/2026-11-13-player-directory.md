# Spielerdatenbank Implementation Plan

**Ziel:** Importierte Spieler verwalten und ihre Matches und PR im Hauptmenü anzeigen.

- [ ] Aliaszuordnung und Konflikte mit einem isolierten PostgreSQL-API-Test absichern.
- [ ] Migration 0052 für Profile, Alias und Matchzuordnung ergänzen. Bestehende Matches und Folgeimporte über dieselbe Zuordnung verarbeiten.
- [ ] REST-Routen in `backend/src/player_directory.rs` ergänzen und den exakten Spielerfilter von `backend/src/databases.rs` für die Matchliste verwenden.
- [ ] API-Typen generieren. `web/src/PlayerDirectory.tsx` in der vorhandenen Hauptnavigation registrieren und die Oberfläche lokalisieren.
- [ ] PostgreSQL-Test, Frontend-Build, Rust- und Frontend-Prüfungen ausführen. Namenspflege und Matchnavigation im Browser prüfen.

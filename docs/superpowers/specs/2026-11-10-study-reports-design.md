# Study report archive

Approved by the user on 2026-11-10: rename Study & Anki to Study and add Reports and Flashcards & Anki, automatically archiving generated reports.

Reuse the existing PostgreSQL play_sessions.state.review as the durable source. Successful review generation already commits the report atomically; old reports require no copying or reanalysis. Add an indexed, paginated read API restricted to completed sessions with a non-null review. Return lightweight summaries ordered newest first and a separate read-only full report endpoint. Reports remain accessible when their library entry is deleted. Loading an archived report must never invoke GNUbg or write to the session.

Study contains two keyboard-accessible tabs: Reports and Flashcards & Anki. Keep the existing flashcard workspace mounted and its selection/progress intact. Open requested decks directly in the flashcard area. Reports shows date, match/player, score/length, mode, assistance, analysis depth and first-attempt PR, with explicit unknown PR, loading, empty, error/retry and load-more states. Selecting a report reuses the full PlayLearningReview component including original attempts and retries. Refresh the archive on re-entry so newly generated reports appear automatically.

All supported interface languages receive labels. Rename navigation references and update affected browser tests. Verify PostgreSQL persistence, old/detached reports, exclusions, pagination beyond 100 sessions, invalid queries and missing reports. Verify browser navigation, report details, reload, refresh, failure/retry, request races and flashcard regression. Run frontend tests/build, targeted Rust tests/format/clippy and browser checks.

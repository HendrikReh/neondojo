# Flashcard Result Actions Implementation Plan

**Goal:** Manage selected cards from the search results and filter cards without a deck.

**Architecture:** The deck picker includes a persistent “Without a deck” scope. Search applies the membership filter before pagination. The results use the existing card and deck identities and version checks. Adding a membership reuses the card, while permanent deletion verifies the selected card versions and memberships in one transaction.

**Tech Stack:** Rust, Axum, PostgreSQL, React, TypeScript, Playwright.

1. Add a database test covering deckless paging, shared membership, deletion conflicts, and preservation of cards outside the selection. Implement the query and membership/deletion routes in the learning module and regenerate the OpenAPI files.
2. Show the selection count and actions above the results in `LearningCardBrowser.tsx` and `LearningPanel.tsx`. Reuse the existing move and membership APIs, use a destination picker for add and move, and confirm permanent deletion with its consequences. Keep selections scoped to the submitted search.
3. Cover the complete browser workflow on a separate database, inspect the three desktop widths, and verify the deployed frontend and backend on port 8787 with isolated test cards. Update the flashcard workflow documentation.

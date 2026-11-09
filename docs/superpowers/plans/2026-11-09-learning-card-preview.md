# Flashcard position preview

**Goal:** A selected card in Study & Anki's deck list shows its question-side board, making cards recognizable without changing review history.

**Design:** Keep the compact list on the left and a single persistent preview on the right, matching the desktop Positions view. Select the first loaded card initially; click, keyboard arrows, Home/End and a focusable question button select another card. Use existing CardQuestion snapshots, player names, GNUbg match context and the active Board theme. Show the decision prompt, dice, score and cube, including pending double/take contexts. Engine answers, played move, comments and review actions remain in the existing study/reveal flow.

Create a focused `LearningCardBrowser.tsx` view and its stylesheet; LearningPanel retains deck loading, pagination, exports and all study/review behavior. Mark fetched pages with their owner deck so a deck switch cannot briefly preview a previous deck's cards. Guard pagination by a deck/revision request identity and abort old requests; retain the selected preview when adding a page. No backend or schema changes, main-data mutations or commit.

- [x] Add failing preview rendering and real-browser interaction tests, including safe question content and deck switching.
- [x] Implement the compact list/preview, six-language labels, theme/context display, keyboard navigation and page-owner guards.
- [x] Verify frontend unit tests/build/format and relevant existing learning/position/Anki browser workflows on a disposable database; review.
- [x] Activate verified static assets without restarting the backend, run a read-only smoke test, and remove disposable resources.

TDD browser failure: the existing deck list had no `.learning-card-preview .board`. After implementation, both focused tests passed: real 3-position analysis/deck browsing without history changes and a 52-card native match-analysis fixture exercising pagination, selection preservation, delayed responses and A→B→A deck switching. A follow-up test clicked the actual question button before pressing ArrowUp; it reproduced that focus remained on the button. Row clicks now focus their row, matching the Positions view, so navigation continues immediately after selecting a question.

Existing regression tests found another issue: the permanently mounted but inactive learning tab created an extra hidden Board SVG during analysis. An added SSR test reproduced the hidden-board failure. App now passes visibility through LearningWorkspace to LearningCardBrowser; the inactive browser keeps its selection but does not instantiate Board. The browser test verifies Analysis→Study navigation retains selection and recreates the preview only when active.

Final verification: 202 frontend tests, production build, formatting and all 27 selected browser regressions pass. These cover flashcard browsing, delayed pagination/deck switching, notes, reviews, checker/double/response questions, native Anki export, themes/languages and independent saved positions. Independent read-only review found no remaining findings; its narrow/mobile-window suggestion was withdrawn in light of the explicitly documented desktop scope.

Activated the verified static assets without restarting the main backend. A read-only smoke test used an existing nonempty deck and checked preview, keyboard navigation and layout at 1800×1169, 2560×1440 and 3840×2160. There were no write requests; card versions, review counts, due dates and settings were unchanged. Removed the disposable test service/database and temporary bundles. No backend/schema changes, commits or private/demo files were added.

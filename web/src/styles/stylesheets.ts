// Every stylesheet of the app, in cascade order. main.tsx imports this module
// before any component, so the order below alone decides which of two equally
// specific rules wins. Components keep importing their own stylesheet to
// document the dependency; those later imports add nothing, whether the
// component is bundled with the app or loaded on demand.
//
// The order reproduces the cascade the app had before workspaces were loaded
// lazily. Global rules (style.css) still come after the components, and many
// component rules rely on that, so moving a file changes computed styles.
// stylesheets.test.ts checks that every stylesheet is listed exactly once.
// ui-appearance.css is linked from index.html ahead of all of them.

import '../shared/ui/action-controls.css';
import '../settings/profiles/profiles.css';
import '../workspaces/extras/benchmark.css';
import '../workspaces/extras/database-backup.css';
import '../workspaces/extras/engine-match.css';
import '../workspaces/extras/version-info.css';
import '../shared/board/board-preview.css';
import '../shared/review/move-history.css';
import '../workspaces/play/game-result.css';
import '../shared/review/cube-decision.css';
import '../shared/review/move-explanation.css';
import '../workspaces/study/reports/learning-review-dialog.css';
import '../workspaces/play/play.css';
import '../workspaces/analysis/board-review.css';
import '../workspaces/positions/position-bookmark.css';
import '../workspaces/analysis/candidates/candidate-table.css';
import '../workspaces/analysis/candidates/position-comparison.css';
import '../workspaces/analysis/candidates/analysis-sources.css';
import '../workspaces/analysis/match/analysis-remaining-time.css';
import '../workspaces/analysis/match/analysis-time-estimate.css';
import '../workspaces/analysis/match/match-statistics.css';
import '../workspaces/study/flashcards/deck-management.css';
import '../workspaces/positions/position-browser.css';
import '../workspaces/study/flashcards/learning-workspace.css';
import '../workspaces/study/flashcards/card-suggestions.css';
import '../workspaces/analysis/analysis-match-header.css';
import '../workspaces/analysis/retained-position.css';
import '../workspaces/analysis/game-plans/game-plan.css';
import '../workspaces/players/player-directory.css';
import '../shared/hooks/resizable-columns.css';
import '../shared/ui/applied-filter-chips.css';
import '../workspaces/library/library.css';
import '../workspaces/study/mistakes/recurring-mistakes.css';
import '../workspaces/study/reports/match-assessment.css';
import '../workspaces/study/study.css';
import '../settings/settings.css';
import '../app/workspace.css';
import '../app/home.css';
import '../workspaces/positions/decision-search.css';
import './style.css';
import './desktop-analysis.css';
import './typography.css';
import './text-navigation.css';

import { useRef, useState } from 'react';
import {
  sessionFields,
  useWorkspaceSession,
} from '../../../shared/hooks/useWorkspaceSession';
import {
  buildStatisticsFilter,
  type DecisionKind,
  type MatchDecisionFilter,
  type StatisticsAction,
} from './match-statistics-api';

export const defaultReviewFilter = {
  player: '',
  severity: '',
  errors_only: true,
};

type SavedReviewFilter = typeof defaultReviewFilter;

/** Visit-scoped filter state for checker review. */
export function useCheckerReviewFilters(visit: number) {
  const [state, setState] = useState<{
    visit: number;
    filter: SavedReviewFilter;
  }>({ visit, filter: defaultReviewFilter });
  return {
    filter: state.visit === visit ? state.filter : defaultReviewFilter,
    setFilter: (filter: SavedReviewFilter) => setState({ visit, filter }),
  };
}

/** Session-persisted filter state for cube review. */
export function useCubeReviewFilters() {
  const [filter, setFilter] = useWorkspaceSession(
    'analysis.cubeReview',
    undefined,
    defaultReviewFilter,
    (value) => sessionFields(value, defaultReviewFilter),
  );
  return { filter, setFilter };
}

export function useMatchReviewFilters({
  visit,
  filterScope,
  reviewKind,
  scoped,
  gameNo,
  onShowReview,
  onGame,
}: {
  visit: number;
  filterScope: string;
  reviewKind: DecisionKind;
  scoped: boolean;
  gameNo?: number;
  onShowReview: (kind: DecisionKind) => void;
  onGame: (gameNo: number) => void;
}) {
  const checker = useCheckerReviewFilters(visit);
  const cube = useCubeReviewFilters();
  const errorsToggle = useRef<HTMLInputElement>(null);
  const resetButton = useRef<HTMLButtonElement>(null);
  const [selectedGames, setSelectedGames] = useState<
    Partial<Record<DecisionKind, { scope: string; game?: number }>>
  >({});

  const selectedGame = selectedGames[reviewKind];
  const selected: MatchDecisionFilter = {
    ...(reviewKind === 'cube' ? cube.filter : checker.filter),
    kind: reviewKind,
    // A single-game job always has this scope, including after reset or a
    // metric action. Keep the visible selection and request explicit.
    game_no: scoped
      ? gameNo
      : selectedGame?.scope === filterScope
        ? selectedGame.game
        : undefined,
  };

  function setSelection(
    kind: DecisionKind,
    filter: Omit<MatchDecisionFilter, 'kind'>,
  ) {
    setSelectedGames((games) => ({
      ...games,
      [kind]: { scope: filterScope, game: filter.game_no },
    }));
    const saved = {
      player: filter.player,
      severity: filter.severity ?? '',
      errors_only: filter.errors_only,
    };
    if (kind === 'cube') cube.setFilter(saved);
    else checker.setFilter(saved);
  }

  const updateFilter = (patch: Partial<Omit<MatchDecisionFilter, 'kind'>>) =>
    setSelection(reviewKind, { ...selected, ...patch });

  function selectMetric(action: StatisticsAction) {
    const filter = buildStatisticsFilter(action);
    const kind = filter.kind === 'cube' ? 'cube' : 'checker';
    setSelection(kind, filter);
    if (action.gameNo !== undefined && action.gameNo !== gameNo)
      onGame(action.gameNo);
    onShowReview(kind);
  }

  return {
    selected,
    updateFilter,
    resetFilters: () => setSelection(reviewKind, defaultReviewFilter),
    removeSelectionFilters: () => {
      setSelection(reviewKind, {
        ...defaultReviewFilter,
        errors_only: selected.errors_only,
      });
      resetButton.current?.focus();
    },
    showAllDecisions: () => {
      updateFilter({ errors_only: false });
      errorsToggle.current?.focus();
    },
    errorsToggle,
    resetButton,
    selectMetric,
  };
}

import { Fragment, type RefObject } from 'react';
import type { Match, Position } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import { errorGrades } from '../../shared/review/move-severity';
import { severityLabels } from '../../shared/review/review-statistics-messages';
import type { MoveAssessment } from '../../shared/review/useMoveAssessments';
import GameResultSummary from '../play/GameResultSummary';
import MoveHistoryRow from './MoveHistoryRow';
import type { CubeAction, GameCubeActions } from './useGameCubeActions';
import type { AnalysisDetail } from './AnalysisWorkspace';
import type { useGameNavigation } from '../../app/useGameNavigation';

type GameItem = ReturnType<typeof useGameNavigation>['games'][number];

export const moveHistoryFilters = ['all', 'errors', ...errorGrades] as const;
export type MoveHistoryFilter = (typeof moveHistoryFilters)[number];
export const parseMoveHistoryFilter = (value: unknown) =>
  moveHistoryFilters.find((filter) => filter === value);

export interface MoveHistoryItem {
  move: Position;
  index: number;
  assessment?: MoveAssessment;
}

export default function AnalysisMoveHistoryPane({
  match,
  game,
  positionsCount,
  history,
  index,
  loading,
  sourceAvailable,
  moveFilter,
  onMoveFilter,
  filteredMoves,
  moveAssessments,
  cubeActions,
  reviewedCubeMove,
  unratedCubeAction,
  onOpenCubeAction,
  moveFilterControlRef,
  moveListRef,
  currentMoveRef,
  inErrorsTab,
  onDetail,
  onSelectIndex,
}: {
  match: Match;
  game?: GameItem;
  positionsCount: number;
  history: MoveHistoryItem[];
  index: number;
  loading: boolean;
  sourceAvailable: boolean;
  moveFilter: MoveHistoryFilter;
  onMoveFilter: (filter: MoveHistoryFilter) => void;
  filteredMoves: boolean;
  moveAssessments: {
    assessments: Record<string, MoveAssessment>;
    loading: boolean;
    error: unknown;
  };
  cubeActions?: GameCubeActions;
  reviewedCubeMove?: string;
  unratedCubeAction?: string;
  onOpenCubeAction: (action: CubeAction) => void;
  moveFilterControlRef: RefObject<HTMLSelectElement | null>;
  moveListRef: RefObject<HTMLDivElement | null>;
  currentMoveRef: RefObject<HTMLButtonElement | null>;
  inErrorsTab: boolean;
  onDetail: (detail: AnalysisDetail) => void;
  onSelectIndex: (index: number) => void;
}) {
  const { t, formatNumber, formatDecision, formatError } = useI18n();

  const cubeLines = (actions?: CubeAction[]) =>
    actions?.map((action) => (
      <button
        type="button"
        key={action.id}
        className={`move-history-cube${action.id === reviewedCubeMove ? ' current' : ''}`}
        aria-current={action.id === reviewedCubeMove ? 'true' : undefined}
        data-cube-action
        onClick={() => onOpenCubeAction(action)}
      >
        <i
          aria-hidden="true"
          className={`player-dot ${action.player === 0 ? 'white' : 'black'}`}
        />
        <span>
          {action.player === 0 ? match.white : match.black}:{' '}
          {formatDecision(action.notation)}
        </span>
        {unratedCubeAction === action.id && (
          <small role="status">{t('matchAnalysis.unavailable')}</small>
        )}
      </button>
    ));

  return (
    <aside className="moves-panel">
      <div className="section-heading">
        <h3>{t('app.moveHistory')}</h3>
        <span>
          {filteredMoves && `${formatNumber(history.length)} / `}
          {formatNumber(game?.position_count ?? 0)}
        </span>
      </div>
      {sourceAvailable && (
        <select
          className="move-history-filter"
          ref={moveFilterControlRef}
          aria-label={t('moveHistory.filter')}
          value={moveFilter}
          disabled={!game?.position_count}
          onChange={(event) => {
            const value = event.target.value;
            onMoveFilter(parseMoveHistoryFilter(value) ?? 'all');
          }}
        >
          <option value="all">{t('moveHistory.all')}</option>
          <option value="errors">{t('moveHistory.errors')}</option>
          {errorGrades.map((grade) => (
            <option key={grade} value={grade}>
              {t(severityLabels[grade])}
            </option>
          ))}
        </select>
      )}
      <div className="moves-list" ref={moveListRef}>
        {history.map(({ move, index: moveIndex, assessment }) => (
          <Fragment key={move.id}>
            {cubeLines(cubeActions?.before.get(move.id))}
            <MoveHistoryRow
              position={move}
              players={[match.white, match.black]}
              assessment={assessment}
              current={moveIndex === index && !reviewedCubeMove}
              buttonRef={moveIndex === index ? currentMoveRef : undefined}
              onSelect={() => {
                // A checker move has no cube review; show its candidates.
                if (inErrorsTab) onDetail('overview');
                onSelectIndex(moveIndex);
              }}
            />
          </Fragment>
        ))}
        {!filteredMoves && cubeLines(cubeActions?.trailing)}
        {filteredMoves && history.length === 0 && !moveAssessments.error && (
          <p className="move-history-status" role="status">
            {loading || moveAssessments.loading
              ? t('moveHistory.loading')
              : positionsCount === 0
                ? t('app.noPositions')
                : Object.keys(moveAssessments.assessments).length === 0
                  ? t('moveHistory.unrated')
                  : t('moveHistory.empty')}
          </p>
        )}
        {!loading && sourceAvailable && game?.result && (
          <GameResultSummary
            result={game.result}
            white={match.white}
            black={match.black}
          />
        )}
      </div>
      {!!moveAssessments.error && (
        <p className="move-assessment-error" role="alert">
          {formatError(moveAssessments.error)}
        </p>
      )}
    </aside>
  );
}

import ActionButton from '../../shared/ui/ActionButton';
import type { Theme } from '../../shared/board/Board';
import type { BoardView } from '../../shared/board/move-preview';
import { useBoardReviewText } from '../../shared/board/board-review-messages';
import type { PreviewCandidate } from '../../shared/review/candidate-preview';
import {
  severityLabels,
  useStatisticsText,
} from '../../shared/review/review-statistics-messages';
import type { Match, Position } from '../../shared/http/api';
import type { useJobs } from '../../shared/jobs/useJobs';
import { useI18n } from '../../shared/i18n/i18n';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import { AddCardsPanel } from '../study/flashcards/AddCardsPanel';
import { useSourceText } from './candidates/analysis-source-messages';
import { BoardDecisionComparison } from './BoardDecisionReview';
import AnalysisPanel from './AnalysisPanel';
import CubeReviewEvaluation from './CubeReviewEvaluation';
import MatchAnalysisPanel from './match/MatchAnalysisPanel';
import MatchDecisionFilters from './match/MatchDecisionFilters';
import type { MatchReview } from './match/useMatchReview';
import NotesPanel from './notes/NotesPanel';
import type { AnnotationKind } from './notes/annotation-api';
import type { MoveNotesTarget } from './notes/MoveNotesDialog';
import type { ReviewDecision } from './review-decisions-api';
import type { AnalysisCatalog } from './useAnalysisCatalog';
import type { PositionCalculation } from './usePositionCalculation';
import type { AnalysisDetail } from './AnalysisWorkspace';
import type { useGameNavigation } from '../../app/useGameNavigation';

type GameItem = ReturnType<typeof useGameNavigation>['games'][number];

export default function AnalysisInspectorPane({
  match,
  games,
  gameNo,
  position,
  gameLoaded,
  loading,
  active,
  detail,
  sourceAvailable,
  theme,
  defaultPlies,
  openingBookRevision,
  deckRevision,
  catalogRevision,
  jobs,
  calculation,
  calculationActive,
  catalog,
  matchReview,
  reviewingCheckers,
  checkerReview,
  cube,
  cubeBoard,
  selectedPreview,
  notesSuspended,
  scopeRequest,
  onCloseCheckerReview,
  onPreview,
  onMoveNotes,
  onNotesSaved,
  onDeckChanged,
}: {
  match: Match;
  games: GameItem[];
  gameNo?: number;
  position?: Position;
  gameLoaded: boolean;
  loading: boolean;
  active: boolean;
  detail: AnalysisDetail;
  sourceAvailable: boolean;
  theme: Theme;
  defaultPlies: number;
  openingBookRevision: number;
  deckRevision: number;
  catalogRevision: string;
  jobs: ReturnType<typeof useJobs>;
  calculation: PositionCalculation;
  calculationActive: boolean;
  catalog: AnalysisCatalog;
  matchReview: MatchReview;
  reviewingCheckers: boolean;
  checkerReview?: ReviewDecision;
  cube?: ReviewDecision;
  cubeBoard?: Position | null;
  selectedPreview?: PreviewCandidate;
  notesSuspended: boolean;
  scopeRequest?: { kind: AnnotationKind; token: number };
  onCloseCheckerReview: () => void;
  onPreview: (candidate: PreviewCandidate | null, view?: BoardView) => void;
  onMoveNotes: (target?: Omit<MoveNotesTarget, 'match'>) => void;
  onNotesSaved: () => void;
  onDeckChanged: () => void;
}) {
  const { t, formatError } = useI18n();
  const w = useWorkspaceText();
  const sourceText = useSourceText();
  const review = useBoardReviewText();
  const statistics = useStatisticsText();
  const overview = detail === 'overview';
  const errors = detail === 'errors';

  return (
    <aside className="position-inspector">
      <div
        className="overview-evaluation"
        role="region"
        aria-label={w('evaluation')}
        hidden={!overview || !!cubeBoard}
      >
        {reviewingCheckers && (
          <div className="checker-statistics-review">
            <div>
              <MatchDecisionFilters
                review={matchReview}
                match={match}
                games={games}
                gameNo={gameNo}
              />
            </div>
            <ActionButton
              type="button"
              icon="clear"
              label={statistics('closeCheckerReview')}
              onClick={onCloseCheckerReview}
            />
            {checkerReview && (
              <BoardDecisionComparison
                played={checkerReview.played}
                best={checkerReview.best}
                context={[
                  checkerReview.evaluation_status === 'evaluated'
                    ? t(severityLabels[checkerReview.severity])
                    : undefined,
                  sourceText(
                    checkerReview.source === 'imported' ? 'imported' : 'engine',
                  ),
                ]
                  .filter(Boolean)
                  .join(' · ')}
                loss={
                  checkerReview.evaluation_status === 'evaluated'
                    ? checkerReview.equity_loss
                    : null
                }
                source={[checkerReview.engine, checkerReview.depth]
                  .filter(Boolean)
                  .join(' · ')}
              />
            )}
          </div>
        )}
        {!!jobs.error && (
          <p className="error" role="alert">
            {formatError(jobs.error)}
          </p>
        )}
        <AnalysisPanel
          positionJobs={jobs}
          calculation={calculation}
          catalog={catalog}
          active={calculationActive}
          resultsVisible={active && overview && !cubeBoard && !loading}
          observedMatchId={sourceAvailable && gameLoaded ? match.id : undefined}
          gameNo={gameNo}
          defaultPlies={defaultPlies}
          positionId={position?.id}
          position={position ?? undefined}
          players={match}
          playerName={
            position
              ? position.player === 0
                ? match.white
                : match.black
              : undefined
          }
          openingBookRevision={openingBookRevision}
          analysisRevision={catalogRevision}
          onPreview={onPreview}
          selectedPreview={selectedPreview}
        />
      </div>
      {sourceAvailable && (
        <div
          className="workspace-match-results"
          hidden={detail !== 'errors' && detail !== 'statistics'}
        >
          <MatchAnalysisPanel
            key={match.id}
            review={matchReview}
            match={match}
            games={games}
            gameNo={gameNo}
            catalog={catalog}
            showResults={errors}
            showStatistics={detail === 'statistics'}
          />
        </div>
      )}
      {cube && cubeBoard && (
        <div className="overview-evaluation">
          <CubeReviewEvaluation decision={cube} players={match} />
        </div>
      )}
      {errors && sourceAvailable && !cubeBoard && (
        <p className="analysis-empty cube-review-unselected">
          {review('cubeUnselected')}
        </p>
      )}
      <div
        id="inspector-panel-notes"
        role="tabpanel"
        aria-labelledby="inspector-tab-notes"
        hidden={detail !== 'notes'}
      >
        <NotesPanel
          key={match.id}
          matchId={match.id}
          gameNo={gameNo}
          onMoveNotes={onMoveNotes}
          suspended={notesSuspended}
          positionId={position?.id}
          onSaved={onNotesSaved}
          scopeRequest={scopeRequest}
        />
      </div>
      <div
        id="inspector-panel-cards"
        role="tabpanel"
        aria-labelledby="inspector-tab-cards"
        hidden={detail !== 'cards'}
      >
        <AddCardsPanel
          positionId={position?.id}
          matchId={match.id}
          analysisId={catalog.analysisId}
          gameNo={gameNo}
          allowBatch={sourceAvailable}
          white={match.white}
          black={match.black}
          theme={theme}
          revision={deckRevision}
          onChanged={onDeckChanged}
        />
      </div>
    </aside>
  );
}

import Board from '../../shared/board/Board';
import type { Theme } from '../../shared/board/themes';
import { useI18n } from '../../shared/i18n/i18n';
import { CubeContextLine } from '../../shared/review/CubeDecisionDetails';
import ActionButton from '../../shared/ui/ActionButton';
import type { DecisionHit } from '../study/mistakes/recurring-mistakes-api';
import { useMistakeText } from '../study/mistakes/recurring-mistakes-messages';
import { mistakeReferenceKey } from '../study/mistakes/recurring-mistakes-provenance';
import DecisionDeckAction, {
  DecisionDeckStatus,
  type DecisionDeckOutcome,
} from './DecisionDeckAction';
import PositionBookmarkButton from './PositionBookmarkButton';
import { useDecisionSearchText } from './decision-search-messages';

const NIL_UUID = '00000000-0000-0000-0000-000000000000';

// A checker decision has a stored position; a cube decision is found in
// its game by its number.
export const hasDecisionPosition = (hit: DecisionHit) =>
  hit.position.id !== NIL_UUID;

export const isDecisionOpenable = (hit: DecisionHit) =>
  hasDecisionPosition(hit) || hit.position.match_uuid !== NIL_UUID;

export default function DecisionPreviewPane({
  preview,
  theme,
  bookmarkRevision,
  onBookmarkChanged,
  deckRevision,
  deckOutcome,
  onDeckAdded,
  onDeckChanged,
  onStudy,
  onOpen,
}: {
  preview: DecisionHit;
  theme: Theme;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  deckRevision: number;
  deckOutcome?: DecisionDeckOutcome;
  onDeckAdded: (outcome: DecisionDeckOutcome) => void;
  onDeckChanged: () => void;
  onStudy: (deckId: string) => void;
  onOpen: (hit: DecisionHit) => void;
}) {
  const t = useDecisionSearchText();
  const mistake = useMistakeText();
  const { formatDecision, formatNumber } = useI18n();
  const loss = formatNumber(preview.equity_loss, {
    minimumFractionDigits: 3,
    maximumFractionDigits: 3,
  });

  return (
    <aside className="position-preview" aria-label={t('preview')}>
      <div className="saved-position-preview-header">
        <div>
          <h3>
            {preview.players[0]} – {preview.players[1]}
          </h3>
          <p className="position-preview-context">
            {t('move')} {preview.reference.game_no}.
            {preview.reference.decision_no} ·{' '}
            {mistake(preview.mistake_kind ?? preview.kind)}
          </p>
        </div>
        {isDecisionOpenable(preview) && (
          <div className="saved-position-preview-actions">
            {hasDecisionPosition(preview) && (
              <PositionBookmarkButton
                positionId={preview.position.id}
                revision={bookmarkRevision}
                onChanged={onBookmarkChanged}
              />
            )}
            <DecisionDeckAction
              references={[preview.reference]}
              outcomeKey={mistakeReferenceKey(preview.reference)}
              revision={deckRevision}
              onAdded={onDeckAdded}
              onDeckCreated={onDeckChanged}
            />
            <ActionButton
              type="button"
              icon="analysis"
              className="primary"
              label={t('openAnalysis')}
              onClick={() => onOpen(preview)}
            />
          </div>
        )}
      </div>
      {deckOutcome?.key === mistakeReferenceKey(preview.reference) && (
        <DecisionDeckStatus outcome={deckOutcome} onStudy={onStudy} />
      )}
      <CubeContextLine
        matchId={preview.position.gnubg_match_id}
        players={preview.players}
        offeredBy={
          preview.kind === 'cube_response'
            ? 1 - preview.position.player
            : undefined
        }
      />
      <Board
        position={{ ...preview.position, notation: '' }}
        white={preview.players[0]}
        black={preview.players[1]}
        theme={theme}
        cubeOfferPending={preview.kind === 'cube_response'}
      />
      <p>
        {t('played')}: {formatDecision(preview.played)} · {t('best')}:{' '}
        {formatDecision(preview.best)} · {t('loss')}: {loss}
      </p>
    </aside>
  );
}

import { useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { engineDisplayName } from './shared/engine/engine-version';
import type { Theme } from './shared/board/themes';
import {
  humanReviewDecisions,
  type ReviewDecisionEntry,
} from './learning-review-positions';
import { useLearningReviewPositions } from './useLearningReviewPositions';
import LearningDecisionPreview from './LearningDecisionPreview';
import { useStudyText } from './study-messages';
import type { PlaySession } from './workspaces/play/play-api';
import { usePlayModeText } from './workspaces/play/play-mode-messages';
import { useI18n } from './shared/i18n/i18n';
import { prDigits } from './shared/format/pr-format';
export default function PlayLearningReview({
  session,
  theme,
}: {
  session: PlaySession;
  theme: Theme;
}) {
  const t = usePlayModeText();
  const studyT = useStudyText();
  const { formatNumber, formatDecision } = useI18n();
  const result = session.review;
  const entries = useMemo(() => humanReviewDecisions(session), [session]);
  const [selected, setSelected] = useState<string | null>(null);
  const rows = useRef(new Map<string, HTMLTableRowElement>());
  const [scrollRequest, setScrollRequest] = useState(0);
  const selectedEntry = entries.find((entry) => entry.key === selected);
  const history = useLearningReviewPositions(session, Boolean(selectedEntry));
  function select(entry: ReviewDecisionEntry) {
    setSelected(entry.key);
    setScrollRequest((value) => value + 1);
    rows.current.get(entry.key)?.focus({ preventScroll: true });
  }
  function rowKey(event: KeyboardEvent<HTMLTableRowElement>, index: number) {
    let next = index;
    if (event.key === 'ArrowDown')
      next = Math.min(entries.length - 1, index + 1);
    else if (event.key === 'ArrowUp') next = Math.max(0, index - 1);
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = entries.length - 1;
    else if (!['Enter', ' '].includes(event.key)) return;
    event.preventDefault();
    select(entries[next]);
    rows.current
      .get(entries[next].key)
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }
  const bot = engineDisplayName(session.engine.id);
  const white = session.human_player === 0 ? session.human_name : bot;
  const black = session.human_player === 1 ? session.human_name : bot;
  if (!result) return null;
  const number = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  return (
    <section className="play-learning-review" aria-label={t('review')}>
      <div className="learning-review-summary">
        <h3>
          {t('review')} · {t(session.play_mode ?? 'competition')}
        </h3>
        <p className="play-mode-help">{studyT('saved')}</p>
        <p>
          <span>{t(session.play_mode ?? 'competition')}</span> ·{' '}
          {t(result.assisted ? 'assisted' : 'independent')} ·{' '}
          {t('analysisDepth')}: {result.analysis_plies}-ply
        </p>
        <p>
          {t('totalLoss')}: {number(result.first_attempt_loss)} ·{' '}
          {t('attempts')}: {result.first_attempt_count} · {t('retry')}:{' '}
          {result.retry_count}
        </p>
        <p className="play-first-pr">
          <strong>
            {t('firstPr')}:{' '}
            {result.first_attempt_rating == null
              ? '—'
              : formatNumber(result.first_attempt_rating, prDigits)}
          </strong>
        </p>
        <p className="play-mode-help">{t('firstPrHelp')}</p>
        {result.independent_first_attempt_count != null && (
          <p>
            {t('independent')}: {result.independent_first_attempt_count} ·{' '}
            {t('assisted')}: {result.assisted_first_attempt_count ?? 0}
          </p>
        )}
      </div>
      <div
        className={`learning-review-content${selectedEntry ? ' learning-review-content--preview' : ''}`}
      >
        {selectedEntry && (
          <LearningDecisionPreview
            key={selectedEntry.key}
            entry={selectedEntry}
            position={history.positions.get(selectedEntry.key)?.position}
            loading={history.loading}
            error={history.error}
            onRetry={history.retry}
            scrollRequest={scrollRequest}
            onBack={() => {
              const row = rows.current.get(selectedEntry.key);
              row?.focus({ preventScroll: true });
              row?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
            }}
            theme={theme}
            white={white}
            black={black}
          />
        )}
        <div
          className="learning-review-decisions"
          tabIndex={0}
          role="region"
          aria-label={studyT('decisions')}
        >
          <div className="play-feedback-results">
            <table>
              <thead>
                <tr>
                  <th>#</th>
                  <th>{t('played')}</th>
                  <th>{t('solution')}</th>
                  <th>Δ</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry, index) => (
                  <tr
                    key={entry.key}
                    ref={(node) => {
                      if (node) rows.current.set(entry.key, node);
                      else rows.current.delete(entry.key);
                    }}
                    tabIndex={
                      selected === entry.key ||
                      (selected === null && index === 0)
                        ? 0
                        : -1
                    }
                    aria-selected={selected === entry.key}
                    className="learning-review-decision"
                    onClick={() => select(entry)}
                    onKeyDown={(event) => rowKey(event, index)}
                  >
                    <td>
                      <button
                        type="button"
                        tabIndex={-1}
                        className="learning-review-decision-number"
                        aria-label={`${studyT('previewDecision')} ${entry.gameNo}.${entry.decision.decision_no}`}
                      >
                        {entry.gameNo}.{entry.decision.decision_no}
                      </button>
                    </td>
                    <td>{formatDecision(entry.decision.played)}</td>
                    <td>{formatDecision(entry.decision.best)}</td>
                    <td>{number(entry.decision.equity_loss)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {session.decisions
            ?.filter((d) => d.original_action != null)
            .map((d) => (
              <details key={d.id}>
                <summary>
                  #{d.game_no ?? '—'}.{(d.history_index ?? 0) + 1} ·{' '}
                  {t('first')}:{' '}
                  {formatDecision(
                    d.original_notation ?? d.original_action ?? '',
                  )}{' '}
                  → {t('played')}:{' '}
                  {formatDecision(d.notation ?? d.action ?? '')} · Δ{' '}
                  {d.first_loss == null ? '—' : number(d.first_loss)} ·{' '}
                  {t('attempts')}: {d.attempts.length} ·{' '}
                  {t(
                    (d.attempts[0]?.assisted ?? d.assisted)
                      ? 'assisted'
                      : 'independent',
                  )}
                </summary>
                <ol>
                  {d.attempts.map((attempt, i) => (
                    <li key={i}>
                      {t(attempt.assisted ? 'assisted' : 'independent')} ·{' '}
                      {formatDecision(attempt.notation ?? attempt.action)} · Δ{' '}
                      {attempt.loss == null ? '—' : number(attempt.loss)}
                    </li>
                  ))}
                </ol>
              </details>
            ))}
        </div>
      </div>
    </section>
  );
}

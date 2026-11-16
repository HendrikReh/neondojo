import { ActionLink } from './ActionButton';
import { memo, useEffect, useMemo, useRef, type ReactNode } from 'react';
import { playSeverity, type PlaySession, type PlayDecision } from './play-api';
import { usePlayText } from './play-messages';
import { useI18n } from './i18n';
import MoveHistoryContent, { moveHistoryTooltip } from './MoveHistoryContent';
import Tooltip from './Tooltip';
import { usePlayModeText } from './play-mode-messages';
import GameResultSummary, { type PlayGameResult } from './GameResultSummary';
import { shortEngineLabel } from './engine-version';

export default memo(function PlayHistory({
  history,
  white,
  black,
  sessionId,
  visible,
  selectedIndex,
  disabled,
  onSelect,
  decisions,
  gameResults,
  reviewAction,
  analysisEngine,
  analysisPlies,
}: {
  history: PlaySession['history'];
  reviewAction?: ReactNode;
  decisions?: PlayDecision[];
  /** Engine version recorded for this session's feedback analysis. */
  analysisEngine?: string | null;
  analysisPlies?: number;
  gameResults?: PlayGameResult[];
  white: string;
  black: string;
  sessionId: string;
  visible: boolean;
  selectedIndex: number | null;
  disabled: boolean;
  onSelect: (index: number) => void;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  const { formatNumber, formatEquity, formatDecision } = useI18n();
  const list = useRef<HTMLOListElement>(null);
  const current = selectedIndex ?? history.length - 1;
  const results = useMemo(
    () =>
      new Map(gameResults?.map((entry) => [entry.history_index, entry.result])),
    [gameResults],
  );
  useEffect(() => {
    if (!visible || !list.current) return;
    const row = list.current.querySelector<HTMLElement>(
      `[data-play-history-index="${current}"]`,
    );
    if (!row) return;
    const outer = list.current.getBoundingClientRect(),
      box = (row.closest('li') ?? row).getBoundingClientRect();
    if (box.top < outer.top) list.current.scrollTop += box.top - outer.top;
    else if (box.bottom > outer.bottom)
      list.current.scrollTop += box.bottom - outer.bottom;
  }, [history.length, sessionId, visible, current, gameResults]);
  return (
    <aside className="play-history moves-panel" aria-label={t('history')}>
      <div className="section-heading">
        <h3>{t('history')}</h3>
        <span>{formatNumber(history.length)}</span>
      </div>
      <ol className="moves-list" ref={list}>
        {history.map((turn, index) => {
          const relevant = decisions?.filter(
            (d) => d.history_index === index && d.first_loss != null,
          );
          const decision =
            relevant?.find(
              (d) =>
                d.original_action === 'move' || d.original_action === 'pass',
            ) ?? relevant?.[0];
          // The row rates the move that was actually played; the first
          // attempt keeps its own loss in the tooltip.
          const loss = decision?.loss ?? decision?.first_loss;
          const severity = decision?.severity ?? decision?.first_severity;
          const firstAttempt = decision
            ? `${modeT('first')}: ${formatDecision(decision.original_notation ?? '')}${decision.first_loss != null ? ` · Δ ${formatEquity(decision.first_loss)}` : ''} · ${modeT((decision.attempts[0]?.assisted ?? decision.assisted) ? 'assisted' : 'independent')}`
            : undefined;
          // Shown under the row when the played move replaced an earlier choice.
          const corrected =
            !!decision?.original_notation &&
            !!decision.notation &&
            decision.original_notation !== decision.notation;
          return (
            <li key={index}>
              <Tooltip
                as="button"
                text={(anchor) => moveHistoryTooltip(anchor, firstAttempt)}
                type="button"
                className={`move-history-row${index === current ? ' current' : ''}${severity ? ` move-assessment-${severity}` : ''}`}
                aria-current={index === current ? 'step' : undefined}
                data-play-history-index={index}
                tabIndex={index === current ? 0 : -1}
                disabled={disabled}
                onClick={(event) => {
                  event.currentTarget.focus({ preventScroll: true });
                  onSelect(index);
                }}
                onKeyDown={(event) => {
                  if (
                    disabled ||
                    !['ArrowUp', 'ArrowDown'].includes(event.key) ||
                    event.altKey ||
                    event.ctrlKey ||
                    event.metaKey ||
                    event.shiftKey
                  )
                    return;
                  event.preventDefault();
                  event.stopPropagation();
                  const next = Math.max(
                    0,
                    Math.min(
                      history.length - 1,
                      index + (event.key === 'ArrowDown' ? 1 : -1),
                    ),
                  );
                  list.current
                    ?.querySelector<HTMLButtonElement>(
                      `[data-play-history-index="${next}"]`,
                    )
                    ?.focus({ preventScroll: true });
                  onSelect(next);
                }}
              >
                <MoveHistoryContent
                  position={turn}
                  assessment={
                    loss != null && severity
                      ? {
                          equity_loss: loss,
                          severity: playSeverity(severity),
                          source: 'native',
                          engine: analysisEngine
                            ? shortEngineLabel(analysisEngine)
                            : 'GNUbg',
                          depth:
                            analysisPlies == null
                              ? undefined
                              : `${analysisPlies}-ply`,
                        }
                      : undefined
                  }
                  players={[white, black]}
                  moveNumber={index + 1}
                />
              </Tooltip>
              {corrected && decision && (
                <small className="play-history-first-attempt">
                  {modeT('first')}:{' '}
                  {formatDecision(decision.original_notation ?? '')}
                  {decision.first_loss != null &&
                    ` · ${formatNumber(decision.first_loss, {
                      minimumFractionDigits: 3,
                      maximumFractionDigits: 3,
                    })}`}
                </small>
              )}
              {results.get(index) && (
                <GameResultSummary
                  result={results.get(index)!}
                  white={white}
                  black={black}
                />
              )}
            </li>
          );
        })}
      </ol>
      <div className="play-history-actions">
        <ActionLink
          icon="export"
          label={t('export')}
          className="play-history-export"
          href={`/api/play/sessions/${encodeURIComponent(sessionId)}/sgf`}
          download="match.sgf"
        >
          {t('export')}
        </ActionLink>
        {reviewAction}
      </div>
    </aside>
  );
});

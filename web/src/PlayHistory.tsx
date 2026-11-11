import { ActionLink } from './ActionButton';
import { memo, useEffect, useMemo, useRef, type ReactNode } from 'react';
import { playSeverity, type PlaySession, type PlayDecision } from './play-api';
import { usePlayText } from './play-messages';
import { useI18n } from './i18n';
import MoveHistoryContent, { moveHistoryTooltip } from './MoveHistoryContent';
import Tooltip from './Tooltip';
import { usePlayModeText } from './play-mode-messages';
import GameResultSummary, { type PlayGameResult } from './GameResultSummary';

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
}: {
  history: PlaySession['history'];
  reviewAction?: ReactNode;
  decisions?: PlayDecision[];
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
  const { formatNumber } = useI18n();
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
          const firstAttempt = decision
            ? `${modeT('first')}: ${decision.original_notation ?? ''} · ${modeT((decision.attempts[0]?.assisted ?? decision.assisted) ? 'assisted' : 'independent')}`
            : undefined;
          return (
            <li key={index}>
              <Tooltip
                as="button"
                text={(anchor) => moveHistoryTooltip(anchor, firstAttempt)}
                type="button"
                className={`move-history-row${index === current ? ' current' : ''}${decision?.first_severity ? ` move-assessment-${decision.first_severity}` : ''}`}
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
                    decision?.first_loss != null && decision.first_severity
                      ? {
                          equity_loss: decision.first_loss,
                          severity: playSeverity(decision.first_severity),
                          source: 'native',
                          engine: 'GNUbg',
                        }
                      : undefined
                  }
                  players={[white, black]}
                  moveNumber={index + 1}
                />
              </Tooltip>
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
          compact
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

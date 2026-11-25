import { useLearningSummary } from './useLearningSummary';
import IndependentOnlyToggle from './mistakes/IndependentOnlyToggle';
import UnanalyzedMatches from './mistakes/UnanalyzedMatches';
import { useEffect, useId, useRef, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { useConfirmedConfiguration } from '../../settings/configuration/ConfigurationProvider';
import MistakeGroupTable from './mistakes/MistakeGroupTable';
import MistakePlayerSelect from './mistakes/MistakePlayerSelect';
import RecurringMistakePractice from './mistakes/RecurringMistakePractice';
import { useGamePlanText } from '../analysis/game-plans/game-plan-messages';
import { useI18n } from '../../shared/i18n/i18n';
import { saveLearningPreferences } from './flashcards/learning-api';
import { useLearningText } from './flashcards/learning-messages';
import {
  loadMistakes,
  type MistakeGroup,
  type MistakePage,
  type MistakePracticeRequest,
} from './mistakes/recurring-mistakes-api';
import { useMistakeText } from './mistakes/recurring-mistakes-messages';
import { useStudyOverviewText } from './study-messages';
import {
  defaultStudyPlayer,
  rankMistakeGroups,
  recommendStudyAction,
} from './study-overview';
import type { Theme } from '../../shared/board/themes';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';

type Props = {
  visible: boolean;
  revision: number;
  theme: Theme;
  onOpenCards: () => void;
  /** Opens the full list for the player the overview shows, if any. */
  onOpenMistakes: (player: string) => void;
  independentOnly: boolean;
  onIndependentOnly: (value: boolean) => void;
  /** Reports the player whose mistakes the overview shows. */
  onPlayer: (player: string) => void;
  /** Opens the player database, where unrated matches are analyzed. */
  onOpenPlayers: () => void;
  onStudyDeck: (id: string) => void;
};

type Result<T> =
  | { status: 'loading' }
  | { status: 'error'; error: unknown }
  | { status: 'ready'; data: T };
const loading = { status: 'loading' } as const;

export default function StudyOverview(props: Props) {
  const confirmed = useConfirmedConfiguration();
  return (
    <Overview
      {...props}
      configuredPlayer={confirmed?.general.player_name ?? ''}
    />
  );
}

function Overview({
  visible,
  revision,
  theme,
  onOpenCards,
  onOpenMistakes,
  onOpenPlayers,
  onPlayer,
  independentOnly,
  onIndependentOnly,
  onStudyDeck,
  configuredPlayer,
}: Props & { configuredPlayer: string }) {
  const t = useStudyOverviewText();
  const mistakeText = useMistakeText();
  const learningText = useLearningText();
  const planText = useGamePlanText();
  const { locale, formatError, formatNumber } = useI18n();
  const { summary, refresh } = useLearningSummary(visible, revision);
  const [limitDraft, setLimitDraft] = useState<string>();
  const [limitError, setLimitError] = useState<unknown>();
  const limitId = useId();
  const limitHint = useId();
  const saveLimit = (saved: number) => {
    const value = Number(limitDraft);
    if (limitDraft === undefined || limitDraft.trim() === '') {
      setLimitDraft(undefined);
      return;
    }
    if (!Number.isInteger(value) || value < 0 || value > 1000) return;
    if (value === saved) {
      setLimitDraft(undefined);
      return;
    }
    setLimitError(undefined);
    void saveLearningPreferences({ new_cards_per_day: value })
      .then(() => {
        setLimitDraft(undefined);
        refresh();
      })
      .catch((error: unknown) => setLimitError(error));
  };
  const [players, setPlayers] = useState<
    | { status: 'loading' }
    | { status: 'error'; error: unknown }
    | { status: 'ready'; data: string[]; resolved: string }
  >(loading);
  const [mistakeRefresh, setMistakeRefresh] = useState(0);
  const [manualPlayer, setManualPlayer] = useState<string>();
  const [mistakes, setMistakes] = useState<{
    player: string;
    result: Result<MistakePage>;
  }>();
  const [practice, setPractice] = useState<MistakePracticeRequest>();
  const trigger = useRef<HTMLElement | null>(null);
  const playerSelect = useRef<HTMLSelectElement>(null);
  const nextHeading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    setPlayers(loading);
    if (!visible) return;
    const abort = new AbortController();
    // The profile may name the player by an alias; the list shows the
    // profile's display name, so the backend translates it.
    void loadMistakes({ resolve: configuredPlayer.trim() }, abort.signal)
      .then(({ players, resolved_player }) => {
        if (!abort.signal.aborted)
          setPlayers({
            status: 'ready',
            data: players,
            resolved: resolved_player ?? '',
          });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setPlayers({ status: 'error', error });
      });
    return () => abort.abort();
  }, [visible, mistakeRefresh, configuredPlayer]);

  const player =
    players.status === 'ready'
      ? manualPlayer === undefined
        ? players.resolved || defaultStudyPlayer(configuredPlayer, players.data)
        : players.data.includes(manualPlayer)
          ? manualPlayer
          : ''
      : '';

  useEffect(() => {
    setPractice(undefined);
    setMistakes(undefined);
    if (!visible || !player) return;
    const abort = new AbortController();
    void loadMistakes(
      { player, ...(independentOnly ? { independent_only: true } : {}) },
      abort.signal,
    )
      .then((data) => {
        if (!abort.signal.aborted)
          setMistakes({ player, result: { status: 'ready', data } });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted)
          setMistakes({ player, result: { status: 'error', error } });
      });
    return () => abort.abort();
  }, [visible, player, independentOnly]);

  useEffect(() => {
    if (player) onPlayer(player);
    // Report the shown player; the callback identity is not a trigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [player]);

  const playerMistakes =
    mistakes?.player === player ? mistakes.result : loading;
  const groups =
    playerMistakes.status === 'ready'
      ? rankMistakeGroups(playerMistakes.data.groups).slice(0, 3)
      : [];
  let mistakeState: Parameters<typeof recommendStudyAction>[1] = players;
  if (players.status === 'ready') {
    if (player)
      mistakeState =
        playerMistakes.status === 'ready'
          ? { status: 'ready', group: groups[0] }
          : playerMistakes;
    else
      mistakeState = {
        status: players.data.length ? 'choose-player' : 'ready',
      };
  }
  const action = recommendStudyAction(
    summary.status === 'ready'
      ? { status: 'ready', deck: summary.data.next_deck }
      : summary,
    mistakeState,
  );
  const mistakeError =
    players.status === 'error'
      ? players
      : playerMistakes.status === 'error'
        ? playerMistakes
        : null;

  function startPractice(group: MistakeGroup, button: HTMLButtonElement) {
    if (!player) return;
    trigger.current = button;
    setPractice({
      player,
      game_plan: group.game_plan,
      mistake_kind: group.mistake_kind,
      ...(independentOnly ? { independent_only: true } : {}),
    });
  }

  return (
    <div className="study-overview">
      <section
        className="study-report-empty study-overview-next"
        aria-labelledby="study-overview-next"
      >
        <h1 id="study-overview-next" ref={nextHeading} tabIndex={-1}>
          {t('next')}
        </h1>
        {action.kind === 'loading' && <p role="status">{t('loading')}</p>}
        {action.kind === 'unavailable' && <p>{t('unavailable')}</p>}
        {action.kind === 'deck' && (
          <>
            <p>{t('deck', { name: action.deck.name })}</p>
            <ActionButton
              className="primary"
              icon="cards"
              label={t('studyDue')}
              onClick={() => onStudyDeck(action.deck.id)}
            />
          </>
        )}
        {action.kind === 'mistake' && (
          <>
            <p>
              {planText(action.group.game_plan)} ·{' '}
              {mistakeText(action.group.mistake_kind)} ·{' '}
              {t('groupMatches', {
                count: formatNumber(action.group.match_count),
              })}
            </p>
            <p>
              {t('groupLoss', {
                loss: formatNumber(action.rates.loss_per_100, {
                  maximumFractionDigits: 3,
                }),
                count: action.rates.decision_count,
              })}
            </p>
            <ActionButton
              className="primary"
              icon="next"
              label={t('practiceGroup')}
              onClick={(event) =>
                startPractice(action.group, event.currentTarget)
              }
            />
          </>
        )}
        {action.kind === 'choose-player' && (
          <>
            <p>{mistakeText('chooseHelp')}</p>
            <ActionButton
              className="primary"
              icon="next"
              label={mistakeText('choose')}
              onClick={() => playerSelect.current?.focus()}
            />
          </>
        )}
        {action.kind === 'cards' && (
          <ActionButton
            className="primary"
            icon="cards"
            label={t('openCards')}
            onClick={onOpenCards}
          />
        )}
      </section>

      <section
        className="study-report-empty study-overview-cards"
        aria-labelledby="study-overview-cards"
      >
        <div className="study-overview-heading">
          <h2 id="study-overview-cards">{t('dueCards')}</h2>
          {action.kind !== 'cards' && (
            <ActionButton
              icon="cards"
              label={t('openCards')}
              onClick={onOpenCards}
            />
          )}
        </div>
        {summary.status === 'loading' && (
          <p role="status">{learningText('loading')}</p>
        )}
        {summary.status === 'error' && (
          <div role="alert">
            <p>{formatError(summary.error)}</p>
            <ActionButton
              icon="refresh"
              label={mistakeText('retry')}
              onClick={refresh}
            />
          </div>
        )}
        {summary.status === 'ready' && (
          <>
            <p className="study-overview-due">
              {formatNumber(summary.data.today_count ?? summary.data.due_count)}
            </p>
            {summary.data.today_count != null && (
              <>
                <p data-study-today>
                  {t('todayBreakdown', {
                    due: summary.data.review_due_count,
                    fresh: summary.data.new_available_today,
                  })}
                </p>
                {summary.data.new_count > summary.data.new_available_today && (
                  <p className="fineprint">
                    {t('newWaiting', {
                      count:
                        summary.data.new_count -
                        summary.data.new_available_today,
                    })}
                  </p>
                )}
                <div className="study-overview-limit">
                  <label htmlFor={limitId}>{t('newPerDay')}</label>
                  <input
                    id={limitId}
                    type="number"
                    inputMode="numeric"
                    min={0}
                    max={1000}
                    step={1}
                    aria-describedby={limitHint}
                    value={limitDraft ?? summary.data.new_cards_per_day}
                    onChange={(event) => setLimitDraft(event.target.value)}
                    onBlur={() => saveLimit(summary.data.new_cards_per_day)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter')
                        saveLimit(summary.data.new_cards_per_day);
                    }}
                  />
                  <span id={limitHint} className="fineprint">
                    {t('newPerDayHint')}
                  </span>
                </div>
                {!!limitError && <p role="alert">{formatError(limitError)}</p>}
              </>
            )}
            {(summary.data.today_count ?? summary.data.due_count) === 0 && (
              <p>
                {summary.data.next_due_at
                  ? t('nextDue', {
                      date: dateTimeFormat(locale, {
                        dateStyle: 'medium',
                        timeStyle: 'short',
                      }).format(new Date(summary.data.next_due_at)),
                    })
                  : t(summary.data.card_count === 0 ? 'emptyCards' : 'noDue')}
              </p>
            )}
          </>
        )}
      </section>

      <section
        className="study-report-empty study-overview-mistakes"
        aria-labelledby="study-overview-mistakes"
      >
        <div className="study-overview-heading">
          <h2 id="study-overview-mistakes">{mistakeText('title')}</h2>
          <ActionButton
            icon="open"
            label={t('openMistakes')}
            onClick={() => onOpenMistakes(player)}
          />
        </div>
        {players.status === 'ready' && (
          <div className="recurring-filters">
            {players.data.length > 0 && (
              <MistakePlayerSelect
                players={players.data}
                value={player}
                selectRef={playerSelect}
                onChange={(name) => {
                  setPractice(undefined);
                  setMistakes(undefined);
                  setManualPlayer(name);
                }}
              />
            )}
            {players.data.length > 0 && (
              <IndependentOnlyToggle
                checked={independentOnly}
                onChange={onIndependentOnly}
              />
            )}
            <ActionButton
              icon="refresh"
              label={mistakeText('refresh')}
              onClick={() => setMistakeRefresh((value) => value + 1)}
            />
          </div>
        )}
        {players.status === 'loading' && (
          <p role="status">{mistakeText('loading')}</p>
        )}
        {players.status === 'ready' && !player && (
          <p>{mistakeText(players.data.length ? 'chooseHelp' : 'noPlayers')}</p>
        )}
        {player && playerMistakes.status === 'loading' && (
          <p role="status">{mistakeText('loading')}</p>
        )}
        {mistakeError && (
          <div role="alert">
            <p>{formatError(mistakeError.error)}</p>
            <ActionButton
              icon="refresh"
              label={mistakeText('retry')}
              onClick={() => setMistakeRefresh((value) => value + 1)}
            />
          </div>
        )}
        {player && playerMistakes.status === 'ready' && (
          <>
            <p>
              {t('coverage', {
                decisions: playerMistakes.data.analyzed_decisions,
                matches: playerMistakes.data.analyzed_matches,
              })}
            </p>
            <UnanalyzedMatches
              page={playerMistakes.data}
              onOpenPlayers={onOpenPlayers}
            />
            {groups.length ? (
              <MistakeGroupTable
                groups={groups}
                onPractice={startPractice}
                compact
              />
            ) : (
              <p>
                {playerMistakes.data.analyzed_decisions
                  ? t('noRecurring')
                  : mistakeText('noCoverage')}
              </p>
            )}
          </>
        )}
      </section>
      {visible && practice?.player === player && (
        <RecurringMistakePractice
          request={practice}
          theme={theme}
          returnFocus={trigger}
          onClose={() => {
            if (!trigger.current?.isConnected)
              trigger.current = nextHeading.current;
            setPractice(undefined);
          }}
        />
      )}
    </div>
  );
}

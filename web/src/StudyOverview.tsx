import { useEffect, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import { useOptionalConfiguration } from './ConfigurationProvider';
import MistakeGroupTable from './MistakeGroupTable';
import MistakePlayerSelect from './MistakePlayerSelect';
import RecurringMistakePractice from './RecurringMistakePractice';
import { useGamePlanText } from './game-plan-messages';
import { useI18n } from './i18n';
import { loadLearningSummary, type LearningSummary } from './learning-api';
import { useLearningText } from './learning-messages';
import {
  loadMistakes,
  type MistakeGroup,
  type MistakePage,
  type MistakePracticeRequest,
} from './recurring-mistakes-api';
import { useMistakeText } from './recurring-mistakes-messages';
import { useStudyOverviewText } from './study-messages';
import {
  defaultStudyPlayer,
  rankMistakeGroups,
  recommendStudyAction,
} from './study-overview';
import type { Theme } from './themes';

type Props = {
  visible: boolean;
  revision: number;
  theme: Theme;
  onOpenCards: () => void;
  onOpenMistakes: () => void;
  onStudyDeck: (id: string) => void;
};

type Result<T> =
  | { status: 'loading' }
  | { status: 'error'; error: unknown }
  | { status: 'ready'; data: T };
const loading = { status: 'loading' } as const;

export default function StudyOverview(props: Props) {
  const profile = useOptionalConfiguration();
  return (
    <Overview
      key={profile?.activeProfile?.id ?? ''}
      {...props}
      configuredPlayer={profile?.current?.general.player_name ?? ''}
    />
  );
}

function Overview({
  visible,
  revision,
  theme,
  onOpenCards,
  onOpenMistakes,
  onStudyDeck,
  configuredPlayer,
}: Props & { configuredPlayer: string }) {
  const t = useStudyOverviewText();
  const mistakeText = useMistakeText();
  const learningText = useLearningText();
  const planText = useGamePlanText();
  const { locale, formatError, formatNumber } = useI18n();
  const [summary, setSummary] = useState<Result<LearningSummary>>(loading);
  const [cardRefresh, setCardRefresh] = useState(0);
  const [players, setPlayers] = useState<Result<string[]>>(loading);
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
    setSummary(loading);
    if (!visible) return;
    const abort = new AbortController();
    let timer: number | undefined;
    void loadLearningSummary(abort.signal)
      .then((data) => {
        if (abort.signal.aborted) return;
        setSummary({ status: 'ready', data });
        if (data.next_due_at) {
          const delay = Date.parse(data.next_due_at) - Date.now();
          timer = window.setTimeout(
            () => setCardRefresh((value) => value + 1),
            delay < 0 ? 15_000 : Math.min(delay + 100, 2_147_483_647),
          );
        }
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setSummary({ status: 'error', error });
      });
    return () => {
      abort.abort();
      window.clearTimeout(timer);
    };
  }, [visible, revision, cardRefresh]);

  useEffect(() => {
    setPlayers(loading);
    if (!visible) return;
    const abort = new AbortController();
    void loadMistakes({}, abort.signal)
      .then(({ players }) => {
        if (!abort.signal.aborted)
          setPlayers({ status: 'ready', data: players });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setPlayers({ status: 'error', error });
      });
    return () => abort.abort();
  }, [visible, mistakeRefresh]);

  const player =
    players.status === 'ready'
      ? manualPlayer === undefined
        ? defaultStudyPlayer(configuredPlayer, players.data)
        : players.data.includes(manualPlayer)
          ? manualPlayer
          : ''
      : '';

  useEffect(() => {
    setPractice(undefined);
    setMistakes(undefined);
    if (!visible || !player) return;
    const abort = new AbortController();
    void loadMistakes({ player }, abort.signal)
      .then((data) => {
        if (!abort.signal.aborted)
          setMistakes({ player, result: { status: 'ready', data } });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted)
          setMistakes({ player, result: { status: 'error', error } });
      });
    return () => abort.abort();
  }, [visible, player]);

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
                count: formatNumber(action.rates.decision_count),
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
              onClick={() => setCardRefresh((value) => value + 1)}
            />
          </div>
        )}
        {summary.status === 'ready' && (
          <>
            <p className="study-overview-due">
              {formatNumber(summary.data.due_count)}
            </p>
            {summary.data.due_count === 0 && (
              <p>
                {summary.data.next_due_at
                  ? t('nextDue', {
                      date: new Intl.DateTimeFormat(locale, {
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
            onClick={onOpenMistakes}
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
                decisions: formatNumber(playerMistakes.data.analyzed_decisions),
                matches: formatNumber(playerMistakes.data.analyzed_matches),
              })}
            </p>
            {groups.length ? (
              <MistakeGroupTable groups={groups} onPractice={startPractice} />
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

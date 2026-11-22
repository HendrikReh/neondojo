import IndependentOnlyToggle from './IndependentOnlyToggle';
import { useDecimalInput } from '../../../shared/format/decimal-input';
import UnanalyzedMatches from './UnanalyzedMatches';
import SearchPagination from '../../../shared/ui/SearchPagination';
import { lastSearchPage } from '../../../shared/hooks/useSearchPages';
import { useEffect, useRef, useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import MistakeGroupTable from './MistakeGroupTable';
import MistakePlayerSelect from './MistakePlayerSelect';
import RecurringMistakePractice from './RecurringMistakePractice';
import { useDatabases } from '../../library/useDatabases';
import type { Theme } from '../../../shared/board/themes';
import { useI18n } from '../../../shared/i18n/i18n';
import { useGamePlanText } from '../../analysis/game-plans/game-plan-messages';
import type { GamePlan } from '../../analysis/game-plans/game-plan-api';
import { useMistakeText } from './recurring-mistakes-messages';
import {
  loadMistakes,
  mistakeGamePlans,
  type MistakeFilters,
  type MistakeGroup,
  type MistakePage,
  type MistakePracticeRequest,
} from './recurring-mistakes-api';
import './recurring-mistakes.css';

export default function RecurringMistakes({
  visible,
  theme,
  requestedPlayer,
  suggestedPlayer,
  independentOnly,
  onIndependentOnly,
  onOpenPlayers,
}: {
  visible: boolean;
  theme: Theme;
  /** A player chosen elsewhere in Study; each new token applies it once. */
  requestedPlayer?: { name: string; token: number };
  /** The overview's player; fills the choice only while it is still empty. */
  suggestedPlayer?: string;
  independentOnly: boolean;
  onIndependentOnly: (value: boolean) => void;
  onOpenPlayers: () => void;
}) {
  const t = useMistakeText(),
    planText = useGamePlanText(),
    { formatError } = useI18n();
  const [player, setPlayer] = useState(''),
    [database, setDatabase] = useState(''),
    [plan, setPlan] = useState<GamePlan | ''>(''),
    [loss, setLoss] = useState('0.08');
  const lossInput = useDecimalInput({
    value: loss,
    onChange: (value) => {
      setPractice(undefined);
      setLoss(value);
    },
    step: 0.01,
    min: 0,
    max: 1000,
  });
  const [page, setPage] = useState<MistakePage>(),
    [players, setPlayers] = useState<string[]>([]);
  const [loading, setLoading] = useState(false),
    [error, setError] = useState<unknown>(),
    [revision, setRevision] = useState(0);
  const [resultPage, setResultPage] = useState(1);
  const [practice, setPractice] = useState<MistakePracticeRequest>();
  const trigger = useRef<HTMLButtonElement | null>(null),
    generation = useRef(0);
  const requestedToken = requestedPlayer?.token;
  useEffect(() => {
    if (!requestedPlayer) return;
    setPractice(undefined);
    setPlayer(requestedPlayer.name);
    // Only a new request applies; later edits of the filter stay the user's.
  }, [requestedToken]);
  useEffect(() => {
    if (visible && suggestedPlayer) setPlayer((own) => own || suggestedPlayer);
  }, [visible, suggestedPlayer]);
  const minLoss = Number(loss),
    valid =
      loss.trim() !== '' &&
      Number.isFinite(minLoss) &&
      minLoss >= 0 &&
      minLoss <= 1000;
  useEffect(() => {
    setPractice(undefined);
    setResultPage(1);
    if (!visible || !valid) return;
    const abort = new AbortController(),
      version = ++generation.current;
    setLoading(true);
    setError(undefined);
    setPage(undefined);
    const filters: MistakeFilters = {
      ...(player ? { player } : {}),
      ...(database ? { database_id: database } : {}),
      ...(plan ? { game_plan: plan } : {}),
      min_loss: minLoss,
      ...(independentOnly ? { independent_only: true } : {}),
    };
    void loadMistakes(filters, abort.signal)
      .then((result) => {
        if (!abort.signal.aborted && version === generation.current) {
          setPlayers(result.players);
          if (player && !result.players.includes(player)) {
            setPlayer('');
            setPage(undefined);
          } else setPage(result);
        }
      })
      .catch((failure) => {
        if (!abort.signal.aborted && version === generation.current)
          setError(failure);
      })
      .finally(() => {
        if (!abort.signal.aborted && version === generation.current)
          setLoading(false);
      });
    return () => {
      abort.abort();
      generation.current++;
    };
  }, [
    visible,
    player,
    database,
    plan,
    minLoss,
    valid,
    revision,
    independentOnly,
  ]);
  const { databases, error: databaseError } = useDatabases(revision, {
    enabled: visible,
  });
  function start(group: MistakeGroup, button: HTMLButtonElement) {
    if (!player || !players.includes(player) || !valid) return;
    trigger.current = button;
    setPractice({
      player,
      ...(database ? { database_id: database } : {}),
      game_plan: group.game_plan,
      min_loss: minLoss,
      mistake_kind: group.mistake_kind,
      ...(independentOnly ? { independent_only: true } : {}),
    });
  }
  const pageSize = 25;
  return (
    <section className="recurring-mistakes" aria-label={t('title')}>
      <div className="study-reports-heading">
        <h1>{t('title')}</h1>
        <ActionButton
          icon="refresh"
          label={t('refresh')}
          disabled={loading}
          onClick={() => setRevision((value) => value + 1)}
        >
          {t('refresh')}
        </ActionButton>
      </div>
      <p className="recurring-help">{t('coverage')}</p>
      <div className="recurring-filters">
        <MistakePlayerSelect
          players={players}
          value={player}
          onChange={(name) => {
            setPractice(undefined);
            setPlayer(name);
          }}
        />
        <label>
          <span>{t('database')}</span>
          <select
            value={database}
            onChange={(event) => {
              setPractice(undefined);
              setDatabase(event.target.value);
            }}
          >
            <option value="">{t('allDatabases')}</option>
            {databases.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{planText('title')}</span>
          <select
            value={plan}
            onChange={(event) => {
              setPractice(undefined);
              setPlan(event.target.value as GamePlan | '');
            }}
          >
            <option value="">{t('any')}</option>
            {mistakeGamePlans.map((item) => (
              <option key={item} value={item}>
                {planText(item)}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{t('minLoss')}</span>
          <input
            {...lossInput.inputProps}
            aria-invalid={!valid}
            onChange={(event) => lossInput.change(event.target.value)}
          />
        </label>
        <IndependentOnlyToggle
          checked={independentOnly}
          onChange={(value) => {
            setPractice(undefined);
            onIndependentOnly(value);
          }}
        />
      </div>
      {!valid && <p role="alert">{t('invalidLoss')}</p>}
      {loading && valid && <p role="status">{t('loading')}</p>}
      {!!(error || databaseError) && (
        <div role="alert">
          <p>{formatError(error || databaseError)}</p>
          <ActionButton
            icon="refresh"
            label={t('retry')}
            onClick={() => setRevision((value) => value + 1)}
          >
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {!player && <p>{t('chooseHelp')}</p>}
      {!loading && !error && page && valid && (
        <>
          {!page.players.length && <p>{t('noPlayers')}</p>}
          {player && (
            <>
              <p>
                {t('analyzed')}: {page.analyzed_decisions} /{' '}
                {page.analyzed_matches}
              </p>
              <UnanalyzedMatches page={page} onOpenPlayers={onOpenPlayers} />
              {!page.groups.length ? (
                <p role="status">
                  {t(page.analyzed_decisions ? 'empty' : 'noCoverage')}
                </p>
              ) : (
                <>
                  <MistakeGroupTable
                    groups={page.groups.slice(
                      (resultPage - 1) * pageSize,
                      resultPage * pageSize,
                    )}
                    onPractice={start}
                  />
                  <SearchPagination
                    page={resultPage}
                    lastPage={lastSearchPage(page.groups.length, pageSize)}
                    onPageChange={setResultPage}
                  />
                </>
              )}
            </>
          )}
        </>
      )}
      {visible && practice && (
        <RecurringMistakePractice
          request={practice}
          theme={theme}
          returnFocus={trigger}
          onClose={() => setPractice(undefined)}
        />
      )}
    </section>
  );
}

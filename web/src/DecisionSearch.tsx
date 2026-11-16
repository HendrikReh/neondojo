import { useEffect, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import { request } from './api';
import type { Database } from './library-api';
import Board from './Board';
import { CubeContextLine } from './CubeDecisionDetails';
import { useDecisionSearchText } from './decision-search-messages';
import EngineLabel from './EngineLabel';
import { useGamePlanText } from './game-plan-messages';
import type { GamePlan } from './game-plan-api';
import { useI18n } from './i18n';
import MistakePlayerSelect from './MistakePlayerSelect';
import RecurringMistakePractice from './RecurringMistakePractice';
import {
  loadMistakes,
  mistakeGamePlans,
  searchDecisions,
  type DecisionFilters,
  type DecisionHit,
  type DecisionSearchPage,
  type MistakeKind,
  type MistakeQuestion,
} from './recurring-mistakes-api';
import { useMistakeText } from './recurring-mistakes-messages';
import {
  mistakeProvenance,
  mistakeReferenceKey,
} from './recurring-mistakes-provenance';
import { SearchFilterInput, SearchFilterSelect } from './SearchFilterForm';
import { NumberedSearchPagination } from './SearchPagination';
import type { Theme } from './themes';
import './decision-search.css';

const PAGE_SIZE = 50;
const NIL_UUID = '00000000-0000-0000-0000-000000000000';
const kinds = ['checker', 'cube_offer', 'cube_response'] as const;
const mistakeKinds: MistakeKind[] = [
  'checker',
  'missed_double',
  'premature_double',
  'wrong_take',
  'wrong_pass',
  'unclassified_cube',
];
const owners = ['self', 'opponent', 'centered'] as const;

type Draft = {
  player: string;
  database: string;
  kind: string;
  mistake: string;
  plan: string;
  minLoss: string;
  maxLoss: string;
  away: string;
  opponentAway: string;
  cubeValue: string;
  cubeOwner: string;
};
const emptyDraft: Draft = {
  player: '',
  database: '',
  kind: '',
  mistake: '',
  plan: '',
  minLoss: '0.05',
  maxLoss: '',
  away: '',
  opponentAway: '',
  cubeValue: '',
  cubeOwner: '',
};

const optional = (text: string) =>
  text.trim() === '' ? undefined : Number(text);

/** Draft text to request filters; undefined while a number is not usable. */
export function decisionFilters(draft: Draft): DecisionFilters | undefined {
  if (!draft.player) return undefined;
  const numbers = {
    min_loss: optional(draft.minLoss),
    max_loss: optional(draft.maxLoss),
    away: optional(draft.away),
    opponent_away: optional(draft.opponentAway),
    cube_value: optional(draft.cubeValue),
  };
  if (Object.values(numbers).some((value) => Number.isNaN(value)))
    return undefined;
  return {
    player: draft.player,
    database_id: draft.database || undefined,
    kind: (draft.kind || undefined) as DecisionFilters['kind'],
    mistake_kind: (draft.mistake || undefined) as MistakeKind | undefined,
    game_plan: (draft.plan || undefined) as GamePlan | undefined,
    cube_owner: (draft.cubeOwner || undefined) as DecisionFilters['cube_owner'],
    ...numbers,
    limit: PAGE_SIZE,
  };
}

type Result =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'error'; error: unknown }
  | { status: 'ready'; page: DecisionSearchPage };

export default function DecisionSearch({
  visible,
  revision,
  theme,
  onOpenPosition,
}: {
  visible: boolean;
  revision: number;
  theme: Theme;
  onOpenPosition: (id: string) => void;
}) {
  const t = useDecisionSearchText();
  const mistake = useMistakeText();
  const plan = useGamePlanText();
  const { formatDecision, formatError, formatNumber } = useI18n();
  const [players, setPlayers] = useState<string[]>([]);
  const [databases, setDatabases] = useState<Database[]>([]);
  const [draft, setDraft] = useState<Draft>(emptyDraft);
  const [applied, setApplied] = useState<{
    filters: DecisionFilters;
    page: number;
    attempt: number;
  }>();
  const [result, setResult] = useState<Result>({ status: 'idle' });
  const [selected, setSelected] = useState<string>();
  const [practice, setPractice] = useState<MistakeQuestion[]>();
  const practiceButton = useRef<HTMLButtonElement>(null);
  const set = (key: keyof Draft) => (value: string) =>
    setDraft((current) => ({ ...current, [key]: value }));

  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    void request<Database[]>('/databases', { signal: abort.signal })
      .then((list) => {
        if (!abort.signal.aborted) setDatabases(list);
      })
      .catch(() => undefined);
    return () => abort.abort();
  }, [visible, revision]);

  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    void loadMistakes(
      { database_id: draft.database || undefined },
      abort.signal,
    )
      .then(({ players }) => {
        if (abort.signal.aborted) return;
        setPlayers(players);
        setDraft((current) =>
          current.player && !players.includes(current.player)
            ? { ...current, player: '' }
            : current,
        );
      })
      .catch(() => undefined);
    return () => abort.abort();
  }, [visible, revision, draft.database]);

  useEffect(() => {
    if (!visible || !applied) return;
    const abort = new AbortController();
    setResult({ status: 'loading' });
    void searchDecisions(
      { ...applied.filters, page: applied.page },
      abort.signal,
    )
      .then((page) => {
        if (abort.signal.aborted) return;
        setResult({ status: 'ready', page });
        setSelected(
          page.items[0] && mistakeReferenceKey(page.items[0].reference),
        );
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setResult({ status: 'error', error });
      });
    return () => abort.abort();
  }, [visible, applied, revision]);

  const filters = decisionFilters(draft);
  const items = result.status === 'ready' ? result.page.items : [];
  const preview =
    items.find((item) => mistakeReferenceKey(item.reference) === selected) ??
    items[0];
  const practicable = items.filter((item) => item.practicable).slice(0, 20);
  const score = (hit: DecisionHit) =>
    hit.away
      ? t('scoreAway', { own: hit.away[0], opponent: hit.away[1] })
      : '—';
  const cube = (hit: DecisionHit) =>
    `${formatNumber(hit.cube_value)}${
      hit.cube_owner == null ? '' : ` · ${hit.players[hit.cube_owner]}`
    }`;
  const loss = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });

  return (
    <section
      className="position-browser decision-search"
      aria-labelledby="decision-search-title"
    >
      <h2 id="decision-search-title">{t('title')}</h2>
      <p className="fineprint">{t('intro')}</p>
      <form
        className="recurring-filters decision-search-filters"
        onSubmit={(event) => {
          event.preventDefault();
          if (filters)
            setApplied((current) => ({
              filters,
              page: 1,
              attempt: (current?.attempt ?? 0) + 1,
            }));
        }}
      >
        <MistakePlayerSelect
          players={players}
          value={draft.player}
          onChange={set('player')}
        />
        <SearchFilterSelect
          label={t('database')}
          value={draft.database}
          onChange={set('database')}
        >
          <option value="">{t('allDatabases')}</option>
          {databases.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </SearchFilterSelect>
        <SearchFilterSelect
          label={t('kind')}
          value={draft.kind}
          onChange={set('kind')}
        >
          <option value="">{t('any')}</option>
          {kinds.map((kind) => (
            <option key={kind} value={kind}>
              {mistake(kind)}
            </option>
          ))}
        </SearchFilterSelect>
        <SearchFilterSelect
          label={t('mistake')}
          value={draft.mistake}
          onChange={set('mistake')}
        >
          <option value="">{t('any')}</option>
          {mistakeKinds.map((kind) => (
            <option key={kind} value={kind}>
              {mistake(kind)}
            </option>
          ))}
        </SearchFilterSelect>
        <SearchFilterSelect
          label={t('plan')}
          value={draft.plan}
          onChange={set('plan')}
        >
          <option value="">{t('any')}</option>
          {mistakeGamePlans.map((name) => (
            <option key={name} value={name}>
              {plan(name)}
            </option>
          ))}
        </SearchFilterSelect>
        <SearchFilterInput
          label={t('minLoss')}
          type="number"
          min={0}
          max={1000}
          step={0.005}
          value={draft.minLoss}
          onChange={set('minLoss')}
        />
        <SearchFilterInput
          label={t('maxLoss')}
          type="number"
          min={0}
          max={1000}
          step={0.005}
          value={draft.maxLoss}
          onChange={set('maxLoss')}
        />
        <SearchFilterInput
          label={t('away')}
          type="number"
          min={1}
          max={64}
          step={1}
          value={draft.away}
          onChange={set('away')}
        />
        <SearchFilterInput
          label={t('opponentAway')}
          type="number"
          min={1}
          max={64}
          step={1}
          value={draft.opponentAway}
          onChange={set('opponentAway')}
        />
        <SearchFilterSelect
          label={t('cubeValue')}
          value={draft.cubeValue}
          onChange={set('cubeValue')}
        >
          <option value="">{t('any')}</option>
          {[1, 2, 4, 8, 16, 32, 64].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </SearchFilterSelect>
        <SearchFilterSelect
          label={t('cubeOwner')}
          value={draft.cubeOwner}
          onChange={set('cubeOwner')}
        >
          <option value="">{t('any')}</option>
          {owners.map((owner) => (
            <option key={owner} value={owner}>
              {t(
                owner === 'self'
                  ? 'ownerSelf'
                  : owner === 'opponent'
                    ? 'ownerOpponent'
                    : 'ownerCentered',
              )}
            </option>
          ))}
        </SearchFilterSelect>
        <ActionButton
          type="submit"
          className="primary"
          icon="search"
          label={t('search')}
          disabled={!filters}
        >
          {t('search')}
        </ActionButton>
      </form>

      {!draft.player && result.status === 'idle' && (
        <p className="position-empty">{t('choosePlayer')}</p>
      )}
      {result.status === 'loading' && <p role="status">{t('loading')}</p>}
      {result.status === 'error' && (
        <div role="alert">
          <p>{formatError(result.error)}</p>
          <ActionButton
            icon="refresh"
            label={mistake('retry')}
            onClick={() =>
              setApplied(
                (current) =>
                  current && { ...current, attempt: current.attempt + 1 },
              )
            }
          >
            {mistake('retry')}
          </ActionButton>
        </div>
      )}
      {result.status === 'ready' && (
        <>
          <div className="decision-search-summary">
            <p role="status">{t('found', { count: result.page.total })}</p>
            <ActionButton
              ref={practiceButton}
              icon="cards"
              label={t('practice')}
              tooltip={t('practiceHint')}
              disabled={practicable.length === 0}
              onClick={() => setPractice(practicable)}
            >
              {t('practice')}
            </ActionButton>
          </div>
          {items.length === 0 ? (
            <p className="position-empty">{t('empty')}</p>
          ) : (
            <div className="position-browser-layout">
              <div>
                <div className="position-table-scroll">
                  <table
                    className="similar-position-table decision-search-table"
                    aria-label={t('title')}
                  >
                    <thead>
                      <tr>
                        <th scope="col">{t('match')}</th>
                        <th scope="col">{t('move')}</th>
                        <th scope="col">{t('kind')}</th>
                        <th scope="col">{t('score')}</th>
                        <th scope="col">{t('cube')}</th>
                        <th scope="col">{t('played')}</th>
                        <th scope="col">{t('best')}</th>
                        <th scope="col">{t('loss')}</th>
                        <th scope="col">{t('source')}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {items.map((hit) => {
                        const key = mistakeReferenceKey(hit.reference);
                        return (
                          <tr
                            key={key}
                            className={
                              hit === preview
                                ? 'similar-position-selected'
                                : undefined
                            }
                          >
                            <td>
                              <button
                                type="button"
                                aria-pressed={hit === preview}
                                onClick={() => setSelected(key)}
                              >
                                {hit.players[0]} – {hit.players[1]}
                              </button>
                            </td>
                            <td>
                              {hit.reference.game_no}.
                              {hit.reference.decision_no}
                            </td>
                            <td>{mistake(hit.mistake_kind ?? hit.kind)}</td>
                            <td>
                              {score(hit)}
                              {hit.crawford && ' · Crawford'}
                            </td>
                            <td>{cube(hit)}</td>
                            <td>{formatDecision(hit.played)}</td>
                            <td>{formatDecision(hit.best)}</td>
                            <td className="decision-search-loss">
                              {loss(hit.equity_loss)}
                            </td>
                            <td>
                              <EngineLabel
                                details={hit.engine}
                                tooltip={mistakeProvenance(hit, mistake)}
                              />
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
                <NumberedSearchPagination
                  page={result.page.page}
                  lastPage={Math.max(
                    1,
                    Math.ceil(result.page.total / result.page.limit),
                  )}
                  onPageChange={(page) =>
                    setApplied((current) => current && { ...current, page })
                  }
                  label={t('title')}
                />
              </div>
              {preview && (
                <aside className="position-preview" aria-label={t('preview')}>
                  <h3>
                    {preview.players[0]} – {preview.players[1]}
                  </h3>
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
                    {t('played')}: {formatDecision(preview.played)} ·{' '}
                    {t('best')}: {formatDecision(preview.best)} · {t('loss')}:{' '}
                    {loss(preview.equity_loss)}
                  </p>
                  {preview.position.id !== NIL_UUID && (
                    <ActionButton
                      type="button"
                      icon="analysis"
                      label={t('openAnalysis')}
                      onClick={() => onOpenPosition(preview.position.id)}
                    >
                      {t('openAnalysis')}
                    </ActionButton>
                  )}
                </aside>
              )}
            </div>
          )}
        </>
      )}
      {visible && practice && (
        <RecurringMistakePractice
          items={practice}
          theme={theme}
          returnFocus={practiceButton}
          onClose={() => setPractice(undefined)}
        />
      )}
    </section>
  );
}

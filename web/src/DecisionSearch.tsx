import { useEffect, useId, useRef, useState } from 'react';
import { useDecimalInput } from './decimal-input';
import ActionButton from './ActionButton';
import { request } from './api';
import type { Database } from './library-api';
import Board from './Board';
import { CubeContextLine } from './CubeDecisionDetails';
import { useDecisionSearchText } from './decision-search-messages';
import EngineLabel from './EngineLabel';
import LibrarySortHeading from './LibrarySortHeading';
import PositionBookmarkButton from './PositionBookmarkButton';
import { usePositionText } from './position-browser-messages';
import { SearchResultRow } from './SearchResultList';
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
  | { status: 'error'; error: unknown }
  | { status: 'ready'; page: DecisionSearchPage };

export default function DecisionSearch({
  visible,
  revision,
  bookmarkRevision,
  onBookmarkChanged,
  theme,
  onOpenPosition,
  onOpenCubeDecision,
}: {
  visible: boolean;
  revision: number;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  theme: Theme;
  onOpenPosition: (id: string) => void;
  /** Opens a cube decision, which has no position of its own, in its game. */
  onOpenCubeDecision: (decision: {
    matchId: string;
    gameNo: number;
    decisionNo: number;
  }) => void;
}) {
  const t = useDecisionSearchText();
  const p = usePositionText();
  const hintId = useId();
  const rowPrefix = `${useId()}-`;
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
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string>();
  const [practice, setPractice] = useState<MistakeQuestion[]>();
  const practiceButton = useRef<HTMLButtonElement>(null);
  // "My player profile" fills the player until one is chosen here.
  const playerChosen = useRef(false);
  const minLoss = useDecimalInput({
    value: draft.minLoss,
    onChange: (value) => set('minLoss')(value),
    step: 0.005,
    min: 0,
    max: 1000,
  });
  const maxLoss = useDecimalInput({
    value: draft.maxLoss,
    onChange: (value) => set('maxLoss')(value),
    step: 0.005,
    min: 0,
    max: 1000,
  });
  const set = (key: keyof Draft) => (value: string) => {
    if (key === 'player') playerChosen.current = true;
    setDraft((current) => ({ ...current, [key]: value }));
  };

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
      .then(({ players, resolved_player }) => {
        if (abort.signal.aborted) return;
        setPlayers(players);
        setDraft((current) => {
          if (!current.player && !playerChosen.current && resolved_player)
            return { ...current, player: resolved_player };
          return current.player && !players.includes(current.player)
            ? { ...current, player: '' }
            : current;
        });
      })
      .catch(() => undefined);
    return () => abort.abort();
  }, [visible, revision, draft.database]);

  useEffect(() => {
    if (!visible || !applied) return;
    const abort = new AbortController();
    // The previous results stay visible while the next ones load.
    setLoading(true);
    void searchDecisions(
      { ...applied.filters, page: applied.page },
      abort.signal,
    )
      .then((page) => {
        if (abort.signal.aborted) return;
        setLoading(false);
        setResult({ status: 'ready', page });
        const keys = page.items.map((item) =>
          mistakeReferenceKey(item.reference),
        );
        setSelected((current) =>
          current && keys.includes(current) ? current : keys[0],
        );
      })
      .catch((error: unknown) => {
        if (abort.signal.aborted) return;
        setLoading(false);
        setResult({ status: 'error', error });
      });
    return () => abort.abort();
  }, [visible, applied, revision]);

  const filters = decisionFilters(draft);
  const items = result.status === 'ready' ? result.page.items : [];
  const preview =
    items.find((item) => mistakeReferenceKey(item.reference) === selected) ??
    items[0];
  const keys = items.map((item) => mistakeReferenceKey(item.reference));
  const direction = applied?.filters.direction ?? 'desc';
  // A checker decision has a stored position; a cube decision is found in
  // its game by its number.
  const hasPosition = (hit: DecisionHit) => hit.position.id !== NIL_UUID;
  const openable = (hit: DecisionHit) =>
    hasPosition(hit) || hit.position.match_uuid !== NIL_UUID;
  function openHit(hit: DecisionHit) {
    if (hasPosition(hit)) onOpenPosition(hit.position.id);
    else if (openable(hit))
      onOpenCubeDecision({
        matchId: hit.position.match_uuid,
        gameNo: hit.reference.game_no,
        decisionNo: hit.reference.decision_no,
      });
  }
  function open(key: string) {
    const hit = items.find(
      (item) => mistakeReferenceKey(item.reference) === key,
    );
    if (hit) openHit(hit);
  }
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
      <h2 id="decision-search-title" className="visually-hidden">
        {t('title')}
      </h2>
      <div className="position-controls position-controls--compact">
        <form
          className="position-filter-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (filters)
              setApplied((current) => ({
                filters: {
                  ...filters,
                  direction: current?.filters.direction,
                },
                page: 1,
                attempt: (current?.attempt ?? 0) + 1,
              }));
          }}
        >
          <div className="position-search-row decision-search-row">
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
            <div className="position-filter-pair">
              <SearchFilterInput
                label={t('minLoss')}
                {...minLoss.inputProps}
                onChange={minLoss.change}
              />
              <SearchFilterInput
                label={t('maxLoss')}
                {...maxLoss.inputProps}
                onChange={maxLoss.change}
              />
            </div>
            <ActionButton
              type="submit"
              className="primary"
              icon="search"
              label={t('search')}
              disabled={!filters}
              aria-describedby={draft.player ? undefined : hintId}
            />
          </div>
          <div className="position-filter-toolbar">
            <details className="position-filter-details">
              <summary>{p('filters')}</summary>
              <div className="position-filter-grid">
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
              </div>
            </details>
            {!draft.player && (
              <p id={hintId} className="decision-search-hint">
                {t('choosePlayer')}
              </p>
            )}
          </div>
        </form>
      </div>

      {loading && result.status !== 'ready' && (
        <p role="status">{t('loading')}</p>
      )}
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
              <div className="position-browser-main" aria-busy={loading}>
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
                        <LibrarySortHeading<'loss'>
                          fields={[{ key: 'loss', label: t('loss') }]}
                          sort="loss"
                          direction={direction}
                          disabled={loading}
                          onSort={() =>
                            setApplied(
                              (current) =>
                                current && {
                                  ...current,
                                  page: 1,
                                  filters: {
                                    ...current.filters,
                                    direction:
                                      direction === 'desc' ? 'asc' : undefined,
                                  },
                                },
                            )
                          }
                        />
                        <th scope="col">{t('source')}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {items.map((hit, index) => {
                        const key = keys[index];
                        return (
                          <SearchResultRow
                            key={key}
                            ids={keys}
                            index={index}
                            idPrefix={rowPrefix}
                            active={hit === preview}
                            selectedClassName="similar-position-selected"
                            aria-selected={hit === preview}
                            onPreview={setSelected}
                            onOpen={open}
                            jumpToEdges
                          >
                            <td>
                              {hit.players[0]} – {hit.players[1]}
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
                              {hit.depth && ` · ${hit.depth}`}
                            </td>
                          </SearchResultRow>
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
                    {openable(preview) && (
                      <div className="saved-position-preview-actions">
                        {hasPosition(preview) && (
                          <PositionBookmarkButton
                            positionId={preview.position.id}
                            revision={bookmarkRevision}
                            onChanged={onBookmarkChanged}
                          />
                        )}
                        <ActionButton
                          type="button"
                          icon="analysis"
                          className="primary"
                          label={t('openAnalysis')}
                          onClick={() => openHit(preview)}
                        />
                      </div>
                    )}
                  </div>
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

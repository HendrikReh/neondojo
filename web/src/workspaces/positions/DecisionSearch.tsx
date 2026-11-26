import { useEffect, useId, useRef, useState } from 'react';
import { useAnalysisErrorThresholds } from '../../settings/configuration/ConfigurationProvider';
import { useDecimalInput } from '../../shared/format/decimal-input';
import ActionButton from '../../shared/ui/ActionButton';
import { useDatabases } from '../library/useDatabases';
import Board from '../../shared/board/Board';
import { CubeContextLine } from '../../shared/review/CubeDecisionDetails';
import { useDecisionSearchText } from './decision-search-messages';
import EngineLabel from '../../shared/engine/EngineLabel';
import LibraryColumnPicker from '../../shared/ui/LibraryColumnPicker';
import LibrarySortHeading from '../../shared/ui/LibrarySortHeading';
import {
  DECISION_COLUMN_IDS,
  DECISION_COLUMN_LAYOUT,
  DECISION_COLUMNS,
  type DecisionColumn,
} from './decision-table-columns';
import { useLibraryText } from '../library/library-messages';
import { useResizableColumns } from '../../shared/hooks/useResizableColumns';
import {
  layoutColumns,
  useWorkspaceLayout,
} from '../../shared/hooks/useWorkspaceSession';
import PositionBookmarkButton from './PositionBookmarkButton';
import DecisionDeckAction, {
  DecisionDeckStatus,
  type DecisionDeckOutcome,
} from './DecisionDeckAction';
import { usePositionText } from './position-browser-messages';
import { SearchResultRow } from '../../shared/ui/SearchResultList';
import { useGamePlanText } from '../analysis/game-plans/game-plan-messages';
import type { GamePlan } from '../analysis/game-plans/game-plan-api';
import { useI18n } from '../../shared/i18n/i18n';
import MistakePlayerSelect from '../study/mistakes/MistakePlayerSelect';
import RecurringMistakePractice from '../study/mistakes/RecurringMistakePractice';
import {
  loadMistakes,
  mistakeGamePlans,
  searchDecisions,
  type DecisionFilters,
  type DecisionHit,
  type DecisionSearchPage,
  type MistakeKind,
  type MistakeQuestion,
} from '../study/mistakes/recurring-mistakes-api';
import { useMistakeText } from '../study/mistakes/recurring-mistakes-messages';
import {
  mistakeProvenance,
  mistakeReferenceKey,
} from '../study/mistakes/recurring-mistakes-provenance';
import {
  SearchFilterInput,
  SearchFilterSelect,
} from '../../shared/ui/SearchFilterForm';
import { NumberedSearchPagination } from '../../shared/ui/SearchPagination';
import type { Theme } from '../../shared/board/themes';
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
  minLoss?: string;
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
  maxLoss: '',
  away: '',
  opponentAway: '',
  cubeValue: '',
  cubeOwner: '',
};

const optional = (text: string) =>
  text.trim() === '' ? undefined : Number(text);

/** Draft text to request filters; undefined while a number is not usable. */
export function decisionFilters(
  draft: Draft,
  profileLoss: number,
): DecisionFilters | undefined {
  if (!draft.player) return undefined;
  const numbers = {
    min_loss:
      draft.minLoss === undefined ? profileLoss : optional(draft.minLoss),
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
  deckRevision,
  onDeckChanged,
  theme,
  onOpenPosition,
  onOpenCubeDecision,
}: {
  visible: boolean;
  revision: number;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  deckRevision: number;
  onDeckChanged: () => void;
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
  const l = useLibraryText();
  const [shownColumns, setShownColumns] = useWorkspaceLayout<DecisionColumn[]>(
    'decisions.visibleColumns',
    undefined,
    [...DECISION_COLUMN_IDS],
    (value) => layoutColumns(value, DECISION_COLUMN_IDS),
  );
  const activeColumns = DECISION_COLUMNS.filter(({ id }) =>
    shownColumns.includes(id),
  );
  const columns = useResizableColumns(DECISION_COLUMN_LAYOUT, {
    visible,
    activeColumns: activeColumns.map(({ id }) => id),
    resizeLabel: (column) => l('resizeColumn', { column }),
  });
  const hintId = useId();
  const rowPrefix = `${useId()}-`;
  const mistake = useMistakeText();
  const plan = useGamePlanText();
  const { formatDecision, formatError, formatNumber } = useI18n();
  const [players, setPlayers] = useState<string[]>([]);
  const profileLoss = useAnalysisErrorThresholds().error;
  const [draft, setDraft] = useState<Draft>(emptyDraft);
  const [applied, setApplied] = useState<{
    filters: DecisionFilters;
    usesProfileLoss: boolean;
    page: number;
    attempt: number;
  }>();
  // Keep applied filters independent of unsubmitted draft edits. A change to
  // their inherited minimum starts a new result set at its first page.
  if (applied?.usesProfileLoss && applied.filters.min_loss !== profileLoss) {
    setApplied({
      ...applied,
      filters: { ...applied.filters, min_loss: profileLoss },
      page: 1,
    });
  }
  const [result, setResult] = useState<Result>({ status: 'idle' });
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string>();
  const [practice, setPractice] = useState<MistakeQuestion[]>();
  const [deckOutcome, setDeckOutcome] = useState<DecisionDeckOutcome>();
  const practiceButton = useRef<HTMLButtonElement>(null);
  // "My player profile" fills the player until one is chosen here.
  const playerChosen = useRef(false);
  // That player's decisions are listed once, on the first visit.
  const autoSearch = useRef(true);
  const minLoss = useDecimalInput({
    value: draft.minLoss ?? String(profileLoss),
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
  // Like Saved positions, Clear resets the filters and refreshes shown
  // results; the chosen player stays because every search needs one.
  function clearFilters() {
    const next = { ...emptyDraft, player: draft.player };
    setDraft(next);
    if (applied) submitDraft(next);
  }
  function submitDraft(next: Draft) {
    const filters = decisionFilters(next, profileLoss);
    if (!filters) return false;
    setApplied((current) => ({
      filters: { ...filters, direction: current?.filters.direction },
      usesProfileLoss: next.minLoss === undefined,
      page: 1,
      attempt: (current?.attempt ?? 0) + 1,
    }));
    return true;
  }
  const set = (key: keyof Draft) => (value: string) => {
    if (key === 'player') playerChosen.current = true;
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const { databases } = useDatabases(revision, { enabled: visible });

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
    if (!visible || applied || !autoSearch.current) return;
    if (playerChosen.current) {
      autoSearch.current = false;
      return;
    }
    if (!draft.player) return;
    if (submitDraft(draft)) autoSearch.current = false;
  }, [visible, applied, draft, profileLoss]);

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

  const filters = decisionFilters(draft, profileLoss);
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
  const pageKey = `page:${keys.join(',')}`;
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

  // The result count and practice action lead the table toolbar, or stand
  // alone above an empty result.
  const summary = result.status === 'ready' && (
    <>
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
      <DecisionDeckAction
        references={items.map((item) => item.reference)}
        outcomeKey={pageKey}
        label={t('addPage')}
        tooltip={t('addPageHint')}
        compact={false}
        revision={deckRevision}
        onAdded={(outcome) => {
          setDeckOutcome(outcome);
          onDeckChanged();
        }}
        onDeckCreated={onDeckChanged}
      />
      {deckOutcome?.key === pageKey && (
        <DecisionDeckStatus outcome={deckOutcome} />
      )}
    </>
  );

  function cell(column: DecisionColumn, hit: DecisionHit) {
    switch (column) {
      case 'match':
        return `${hit.players[0]} – ${hit.players[1]}`;
      case 'move':
        return `${hit.reference.game_no}.${hit.reference.decision_no}`;
      case 'kind':
        return mistake(hit.mistake_kind ?? hit.kind);
      case 'score':
        return `${score(hit)}${hit.crawford ? ' · Crawford' : ''}`;
      case 'cube':
        return cube(hit);
      case 'played':
        return formatDecision(hit.played);
      case 'best':
        return formatDecision(hit.best);
      case 'loss':
        return loss(hit.equity_loss);
      case 'source':
        return (
          <>
            <EngineLabel
              details={hit.engine}
              tooltip={mistakeProvenance(hit, mistake)}
            />
            {hit.depth && ` · ${hit.depth}`}
          </>
        );
    }
  }

  return (
    <section
      className="position-browser decision-search"
      aria-labelledby="decision-search-title"
    >
      <div className="position-browser-heading">
        <h1 id="decision-search-title">{t('title')}</h1>
      </div>
      <div className="position-controls position-controls--compact">
        <form
          className="position-filter-form"
          onSubmit={(event) => {
            event.preventDefault();
            submitDraft(draft);
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
            <ActionButton
              icon="clear"
              label={p('clear')}
              compact
              type="button"
              onClick={clearFilters}
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

      {result.status === 'idle' && !loading && !!draft.player && (
        <p className="position-empty decision-search-idle" role="status">
          {t('idle', { action: t('search'), player: draft.player })}
        </p>
      )}
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
          {items.length === 0 ? (
            <>
              <div className="decision-search-summary">{summary}</div>
              <p className="position-empty">{t('empty')}</p>
            </>
          ) : (
            <div className="position-browser-layout">
              <div className="position-browser-main" aria-busy={loading}>
                <div className="position-page-select decision-table-toolbar">
                  <div className="decision-search-summary">{summary}</div>
                  {!shownColumns.includes('loss') && (
                    <span className="library-hidden-sort">
                      {l('sortedBy', {
                        column: t('loss'),
                        direction: l(
                          direction === 'asc' ? 'ascending' : 'descending',
                        ),
                      })}
                    </span>
                  )}
                  <LibraryColumnPicker
                    options={DECISION_COLUMN_IDS.map((id) => ({
                      id,
                      label: t(id),
                    }))}
                    shown={shownColumns}
                    onToggle={(id, checked) =>
                      setShownColumns((current) => {
                        const next = DECISION_COLUMN_IDS.filter((column) =>
                          column === id ? checked : current.includes(column),
                        );
                        return next.length ? next : current;
                      })
                    }
                    onAll={() => setShownColumns([...DECISION_COLUMN_IDS])}
                    onReset={() => setShownColumns([...DECISION_COLUMN_IDS])}
                  />
                  <ActionButton
                    icon="reset"
                    label={l('resetColumnWidths')}
                    compact
                    type="button"
                    className="position-width-reset"
                    onClick={columns.reset}
                    disabled={!columns.custom}
                  />
                </div>
                <div className="position-table-scroll">
                  <table
                    ref={columns.tableRef}
                    className={`position-table saved-position-table decision-search-table${columns.dragging ? ' resizing-table' : ''}`}
                    style={
                      columns.tableStyle ?? {
                        minWidth: activeColumns.reduce(
                          (width, column) => width + column.defaultWidth,
                          0,
                        ),
                      }
                    }
                    aria-label={t('title')}
                  >
                    <colgroup>
                      {activeColumns.map((column) => (
                        <col
                          key={column.id}
                          data-column={column.id}
                          className={column.className}
                          style={{
                            width:
                              columns.widths?.[column.id] ??
                              column.defaultWidth,
                          }}
                        />
                      ))}
                    </colgroup>
                    <thead>
                      <tr>
                        {activeColumns.map(({ id }) =>
                          id === 'loss' ? (
                            <LibrarySortHeading<'loss'>
                              key={id}
                              fields={[{ key: 'loss', label: t('loss') }]}
                              sort="loss"
                              direction={direction}
                              disabled={loading}
                              column={id}
                              resizeHandle={columns.handle(id, t(id))}
                              onSort={() =>
                                setApplied(
                                  (current) =>
                                    current && {
                                      ...current,
                                      page: 1,
                                      filters: {
                                        ...current.filters,
                                        direction:
                                          direction === 'desc'
                                            ? 'asc'
                                            : undefined,
                                      },
                                    },
                                )
                              }
                            />
                          ) : (
                            <th scope="col" key={id} data-column={id}>
                              {t(id)}
                              {columns.handle(id, t(id))}
                            </th>
                          ),
                        )}
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
                            {activeColumns.map(({ id }) => (
                              <td
                                key={id}
                                className={
                                  id === 'loss'
                                    ? 'decision-search-loss'
                                    : undefined
                                }
                              >
                                {cell(id, hit)}
                              </td>
                            ))}
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
                        <DecisionDeckAction
                          references={[preview.reference]}
                          outcomeKey={mistakeReferenceKey(preview.reference)}
                          revision={deckRevision}
                          onAdded={(outcome) => {
                            setDeckOutcome(outcome);
                            onDeckChanged();
                          }}
                          onDeckCreated={onDeckChanged}
                        />
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
                  {deckOutcome?.key ===
                    mistakeReferenceKey(preview.reference) && (
                    <DecisionDeckStatus outcome={deckOutcome} />
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

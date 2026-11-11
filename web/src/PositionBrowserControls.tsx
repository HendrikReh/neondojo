import ActionButton from './ActionButton';
import SearchFilterForm, {
  SearchFilterInput,
  SearchFilterSelect,
} from './SearchFilterForm';
import { useGamePlanText } from './game-plan-messages';
import { mistakeGamePlans } from './recurring-mistakes-api';
import { useEffect, useRef, useState } from 'react';
import { useI18n } from './i18n';
import type { Database } from './library-api';
import {
  deletePositionFilter,
  loadPositionFilters,
  savePositionFilter,
  positionDefinition,
  samePositionDefinition,
  type PositionFilterDraft,
  type SavedPositionFilter,
} from './position-browser-api';
import {
  usePositionText,
  type PositionMessageKey,
} from './position-browser-messages';

export default function PositionBrowserControls({
  filters,
  databases,
  onChange,
  onSearch,
  onClear,
  onLoad,
  busy,
  revision,
}: {
  filters: PositionFilterDraft;
  databases: Database[];
  onChange: (filters: PositionFilterDraft) => void;
  onSearch: () => void;
  onClear: () => void;
  onLoad: (saved: SavedPositionFilter) => void;
  busy: boolean;
  revision: number;
}) {
  const p = usePositionText();
  const planText = useGamePlanText();
  const field = (
    key: keyof PositionFilterDraft,
    label: PositionMessageKey,
    type = 'text',
    min = 0,
    max?: number,
  ) => (
    <SearchFilterInput
      label={p(label)}
      type={type}
      min={type === 'number' ? min : undefined}
      max={max}
      step={
        key === 'min_loss' || key === 'max_loss'
          ? 'any'
          : type === 'number'
            ? 1
            : undefined
      }
      value={filters[key]}
      onChange={(value) => onChange({ ...filters, [key]: value })}
    />
  );
  const select = (
    key: keyof PositionFilterDraft,
    label: PositionMessageKey,
    options: [string, PositionMessageKey][],
  ) => (
    <SearchFilterSelect
      label={p(label)}
      value={filters[key]}
      onChange={(value) => onChange({ ...filters, [key]: value })}
    >
      {options.map(([value, text]) => (
        <option key={value} value={value}>
          {p(text)}
        </option>
      ))}
    </SearchFilterSelect>
  );
  const databaseMissing =
    !!filters.database_id &&
    !databases.some((database) => database.id === filters.database_id);
  return (
    <SearchFilterForm
      filters={filters}
      onChange={onChange}
      onSearch={onSearch}
      onClear={onClear}
      searchLabel={p('search')}
      searchDisabled={busy}
      savedSearches={
        <SavedPositionSearches
          filters={filters}
          onLoad={onLoad}
          revision={revision}
        />
      }
      scope={
        <SearchFilterSelect
          label={p('database')}
          value={filters.database_id}
          onChange={(database_id) => onChange({ ...filters, database_id })}
        >
          <option value="">{p('allDatabases')}</option>
          {databaseMissing && (
            <option value={filters.database_id}>
              {p('missing')} ({filters.database_id})
            </option>
          )}
          {databases.map((database) => (
            <option key={database.id} value={database.id}>
              {database.name}
            </option>
          ))}
        </SearchFilterSelect>
      }
      beforeTextFilters={
        <>
          <SearchFilterSelect
            label={planText('title')}
            value={filters.game_plan ?? ''}
            onChange={(game_plan) => onChange({ ...filters, game_plan })}
          >
            <option value="">{p('any')}</option>
            {mistakeGamePlans.map((plan) => (
              <option key={plan} value={plan}>
                {planText(plan)}
              </option>
            ))}
          </SearchFilterSelect>
          {field('match_name', 'matchName')}
          {field('tournament', 'tournament')}
          {field('round', 'round')}
        </>
      }
      footer={
        <>
          {select('sort', 'sort', [
            ['match', 'sortMatch'],
            ['loss', 'sortLoss'],
            ['match_id', 'matchId'],
            ['database', 'database'],
            ['game', 'gameNumber'],
            ['ply', 'moveNumber'],
            ['player', 'player'],
            ['opponent', 'opponent'],
            ['dice', 'dice'],
            ['score', 'score'],
            ['cube', 'cube'],
            ['played_move', 'playedMove'],
            ['decks', 'decks'],
            ['tags', 'tag'],
            ['comment', 'comment'],
          ])}
          {select('direction', 'direction', [
            ['', 'sortDefault'],
            ['asc', 'ascending'],
            ['desc', 'descending'],
          ])}
        </>
      }
    >
      {field('match_points_min', 'minPoints', 'number')}
      {field('match_points_max', 'maxPoints', 'number')}
      {field('die1', 'die1', 'number', 1, 6)}
      {field('die2', 'die2', 'number', 1, 6)}
      {field('white_score', 'whiteScore', 'number')}
      {field('black_score', 'blackScore', 'number')}
      {select('crawford', 'crawford', [
        ['', 'any'],
        ['true', 'yes'],
        ['false', 'no'],
      ])}
      {field('cube_value', 'cube', 'number', 1, 32768)}
      {select('analysis_state', 'analysis', [
        ['', 'any'],
        ['ready', 'ready'],
        ['missing', 'missing'],
      ])}
      {field('min_loss', 'minLoss', 'number')}
      {field('max_loss', 'maxLoss', 'number')}
    </SearchFilterForm>
  );
}

function SavedPositionSearches({
  filters,
  onLoad,
  revision,
}: {
  filters: PositionFilterDraft;
  onLoad: (saved: SavedPositionFilter) => void;
  revision: number;
}) {
  const p = usePositionText(),
    { formatError } = useI18n();
  const [items, setItems] = useState<SavedPositionFilter[]>([]),
    [active, setActive] = useState<SavedPositionFilter>(),
    [chosen, setChosen] = useState(''),
    [name, setName] = useState(''),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(),
    [invalid, setInvalid] = useState(false),
    [notice, setNotice] = useState<'saved' | 'deleted'>(),
    [reload, setReload] = useState(0);
  const live = useRef(true),
    generation = useRef(0);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      generation.current++;
    };
  }, []);
  useEffect(() => {
    const abort = new AbortController(),
      version = generation.current;
    void loadPositionFilters(abort.signal)
      .then((data) => {
        if (!abort.signal.aborted && version === generation.current) {
          setItems(data);
          setError(undefined);
        }
      })
      .catch((error) => {
        if (!abort.signal.aborted) setError(error);
      });
    return () => abort.abort();
  }, [revision, reload]);
  let changed = false;
  try {
    changed =
      !!active &&
      !samePositionDefinition(active.definition, positionDefinition(filters));
  } catch {
    changed = !!active;
  }
  function load() {
    const item = items.find((item) => item.id === chosen);
    if (!item) return;
    setActive(item);
    setName(item.name);
    setError(undefined);
    setInvalid(false);
    setNotice(undefined);
    onLoad(item);
  }
  async function save(update: boolean) {
    let definition;
    try {
      definition = positionDefinition(filters);
    } catch {
      setInvalid(true);
      return;
    }
    const version = ++generation.current;
    setBusy(true);
    setError(undefined);
    setInvalid(false);
    setNotice(undefined);
    try {
      const item = await savePositionFilter(
        name.trim(),
        definition,
        update ? active : undefined,
      );
      if (!live.current || version !== generation.current) return;
      setItems((previous) =>
        [...previous.filter((other) => other.id !== item.id), item].sort(
          (a, b) => a.name.localeCompare(b.name),
        ),
      );
      setActive(item);
      setChosen(item.id);
      setName(item.name);
      setNotice('saved');
      setReload((value) => value + 1);
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setBusy(false);
    }
  }
  async function remove() {
    if (!active) return;
    const item = active,
      version = ++generation.current;
    setBusy(true);
    setError(undefined);
    setNotice(undefined);
    try {
      await deletePositionFilter(item);
      if (!live.current || version !== generation.current) return;
      setItems((previous) => previous.filter((other) => other.id !== item.id));
      setActive(undefined);
      setChosen('');
      setName('');
      setNotice('deleted');
      setReload((value) => value + 1);
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setBusy(false);
    }
  }
  return (
    <details className="position-saved-searches">
      <summary>{p('savedSearches')}</summary>
      <div className="position-saved-controls">
        <label>
          <span>{p('chooseSearch')}</span>
          <select
            value={chosen}
            disabled={busy}
            onChange={(event) => {
              setChosen(event.target.value);
              setNotice(undefined);
            }}
          >
            <option value="">—</option>
            {items.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
                {item.database_missing ? ` · ${p('missing')}` : ''}
              </option>
            ))}
          </select>
        </label>
        <ActionButton
          icon="open"
          label={p('loadSearch')}
          compact
          disabled={!chosen || busy}
          onClick={load}
        />
        <label>
          <span>{p('searchName')}</span>
          <input
            value={name}
            maxLength={120}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <ActionButton
          icon="save"
          label={p('saveNew')}
          disabled={!name.trim() || busy}
          onClick={() => void save(false)}
        />
        <ActionButton
          icon="save"
          label={p('updateSearch')}
          compact
          disabled={!active || !name.trim() || busy}
          onClick={() => void save(true)}
        />
        <ActionButton
          icon="delete"
          label={p('deleteSearch')}
          compact
          disabled={!active || busy}
          onClick={() => void remove()}
        />
        <ActionButton
          icon="refresh"
          label={p('reload')}
          compact
          disabled={busy}
          onClick={() => setReload((value) => value + 1)}
        />
      </div>
      {active?.database_missing && <p role="alert">{p('databaseMissing')}</p>}
      {changed && <p className="position-unsaved">{p('dirty')}</p>}
      {invalid && <p role="alert">{p('invalidFilter')}</p>}
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
      {busy && <p role="status">{p('saving')}</p>}
      {notice && <p role="status">{p(notice)}</p>}
    </details>
  );
}

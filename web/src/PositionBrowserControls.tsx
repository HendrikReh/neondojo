import ActionButton from './shared/ui/ActionButton';
import AppliedFilterChips from './shared/ui/AppliedFilterChips';
import SearchFilterForm, {
  SearchFilterInput,
  SearchFilterSelect,
  searchTextFields,
} from './shared/ui/SearchFilterForm';
import { useGamePlanText } from './workspaces/analysis/game-plans/game-plan-messages';
import { mistakeGamePlans } from './recurring-mistakes-api';
import { useEffect, useId, useRef, useState } from 'react';
import { DeckDialog } from './DeckLibraryDialogs';
import { usePopoverPlacement } from './shared/hooks/usePopoverPlacement';
import { useI18n } from './shared/i18n/i18n';
import type { Database } from './library-api';
import {
  deletePositionFilter,
  loadPositionFilters,
  savePositionFilter,
  positionDefinition,
  positionFilterKeys,
  samePositionDefinition,
  type PositionFilterDraft,
  type PositionFilterKey,
  type SavedPositionFilter,
} from './position-browser-api';
import {
  usePositionText,
  type PositionMessageKey,
} from './position-browser-messages';

const filterLabels = {
  q: searchTextFields.q.label,
  player: searchTextFields.player.label,
  tag: searchTextFields.tag.label,
  comment: searchTextFields.comment.label,
  position_id: searchTextFields.position_id.label,
  database_id: 'database',
  match_name: 'matchName',
  tournament: 'tournament',
  round: 'round',
  match_points_min: 'minPoints',
  match_points_max: 'maxPoints',
  die1: 'die1',
  die2: 'die2',
  white_score: 'whiteScore',
  black_score: 'blackScore',
  crawford: 'crawford',
  cube_value: 'cube',
  analysis_state: 'analysis',
  min_loss: 'minLoss',
  max_loss: 'maxLoss',
} as const satisfies Record<
  Exclude<PositionFilterKey, 'game_plan'>,
  PositionMessageKey
>;

const analysisOptions = [
  ['', 'any'],
  ['ready', 'ready'],
  ['missing', 'missing'],
] as const;

export default function PositionBrowserControls({
  filters,
  applied,
  activeKeys,
  databases,
  onChange,
  onSearch,
  onClear,
  onRemoveFilter,
  onLoad,
  busy,
  revision,
}: {
  filters: PositionFilterDraft;
  applied: PositionFilterDraft;
  activeKeys: readonly PositionFilterKey[];
  databases: Database[];
  onChange: (filters: PositionFilterDraft) => void;
  onSearch: () => void;
  onClear: () => void;
  onRemoveFilter: (key: PositionFilterKey) => void;
  onLoad: (saved: SavedPositionFilter) => void;
  busy: boolean;
  revision: number;
}) {
  const p = usePositionText();
  const planText = useGamePlanText();
  const { formatNumber } = useI18n();
  const chips = activeKeys.flatMap<{ key: PositionFilterKey; text: string }>(
    (key) => {
      if (key === 'die2' && applied.die1) return [];
      if (key === 'die1' || key === 'die2') {
        const dice = [applied.die1, applied.die2]
          .filter(Boolean)
          .map((die) => formatNumber(Number(die)))
          .join('–');
        return [
          { key, text: p('appliedFilter', { label: p('dice'), value: dice }) },
        ];
      }
      const label =
        key === 'game_plan' ? planText('title') : p(filterLabels[key]);
      return [
        { key, text: p('appliedFilter', { label, value: filterValue(key) }) },
      ];
    },
  );
  const canReset =
    activeKeys.length > 0 ||
    positionFilterKeys.some((key) => filters[key] !== '') ||
    applied.sort !== 'match' ||
    !!applied.direction ||
    filters.sort !== 'match' ||
    !!filters.direction;

  function filterValue(key: PositionFilterKey) {
    const value = applied[key];
    switch (key) {
      case 'database_id':
        return (
          databases.find((database) => database.id === value)?.name ??
          `${p('missing')} (${value})`
        );
      case 'crawford':
        return p(value === 'true' ? 'yes' : 'no');
      case 'analysis_state':
        return p(value === 'ready' ? 'ready' : 'missing');
      case 'game_plan': {
        const plan = mistakeGamePlans.find((plan) => plan === value);
        return plan ? planText(plan) : value;
      }
      case 'position_id':
      case 'q':
      case 'player':
      case 'match_name':
      case 'tournament':
      case 'round':
      case 'tag':
      case 'comment':
        return value;
      default:
        return formatNumber(Number(value), { maximumFractionDigits: 20 });
    }
  }

  const field = (
    key: keyof typeof filterLabels,
    type = 'text',
    min = 0,
    max?: number,
  ) => (
    <SearchFilterInput
      label={p(filterLabels[key])}
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
    options: readonly (readonly [string, PositionMessageKey])[],
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
  const saved = useSavedPositionSearches({ filters, onLoad, revision });
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
      compactToolbar={{
        reset: canReset,
        appliedFilters: (
          <AppliedFilterChips
            chips={chips}
            disabled={busy}
            onRemove={(key, button) => {
              button.form
                ?.querySelector<HTMLElement>(
                  '.position-filter-details > summary',
                )
                ?.focus();
              onRemoveFilter(key);
            }}
          />
        ),
      }}
      searchRowExtra={saved.field}
      savedSearches={saved.status}
      scope={
        <SearchFilterSelect
          label={p(filterLabels.database_id)}
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
      filterGroups={(text) => (
        <div className="position-filter-groups">
          <fieldset className="position-filter-group">
            <legend>{p('groupMatch')}</legend>
            <div className="position-filter-grid">
              {field('match_name')}
              {field('tournament')}
              {field('round')}
              <div className="position-filter-pair">
                {field('match_points_min', 'number')}
                {field('match_points_max', 'number')}
              </div>
            </div>
          </fieldset>
          <fieldset className="position-filter-group">
            <legend>{p('groupPosition')}</legend>
            <div className="position-filter-grid">
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
              <div className="position-filter-pair">
                {field('die1', 'number', 1, 6)}
                {field('die2', 'number', 1, 6)}
              </div>
              {field('white_score', 'number')}
              {field('black_score', 'number')}
              {select('crawford', filterLabels.crawford, [
                ['', 'any'],
                ['true', 'yes'],
                ['false', 'no'],
              ])}
              {field('cube_value', 'number', 1, 32768)}
              {text.position_id}
            </div>
          </fieldset>
          <fieldset className="position-filter-group position-filter-group--half">
            <legend>{p('groupAnalysis')}</legend>
            <div className="position-filter-grid">
              {select(
                'analysis_state',
                filterLabels.analysis_state,
                analysisOptions,
              )}
              <div className="position-filter-pair">
                {field('min_loss', 'number')}
                {field('max_loss', 'number')}
              </div>
            </div>
          </fieldset>
          <fieldset className="position-filter-group position-filter-group--half">
            <legend>{p('groupNotes')}</legend>
            <div className="position-filter-grid">
              {text.tag}
              {text.comment}
            </div>
          </fieldset>
        </div>
      )}
    />
  );
}

export function useSavedPositionSearches({
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
    [deleted, setDeleted] = useState<SavedPositionFilter>(),
    [saving, setSaving] = useState(false),
    [menuOpen, setMenuOpen] = useState(false),
    [reload, setReload] = useState(0);
  const menuId = useId(),
    menu = useRef<HTMLDivElement>(null),
    menuButton = useRef<HTMLButtonElement>(null);
  usePopoverPlacement(menuOpen, menuButton, menu);
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
  function choose(id: string) {
    const item = items.find((item) => item.id === id);
    setChosen(item?.id ?? '');
    setNotice(undefined);
    setActive(item);
    if (!item) return;
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
      setSaving(false);
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
      setDeleted(item);
      setReload((value) => value + 1);
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setBusy(false);
    }
  }
  async function restore() {
    if (!deleted) return;
    const item = deleted,
      version = ++generation.current;
    setBusy(true);
    setError(undefined);
    setNotice(undefined);
    try {
      const saved = await savePositionFilter(item.name, item.definition);
      if (!live.current || version !== generation.current) return;
      setItems((previous) =>
        [...previous.filter((other) => other.id !== saved.id), saved].sort(
          (a, b) => a.name.localeCompare(b.name),
        ),
      );
      setDeleted(undefined);
      setChosen(saved.id);
      setNotice('saved');
      setReload((value) => value + 1);
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setBusy(false);
    }
  }
  function closeMenu() {
    menu.current?.hidePopover();
    menuButton.current?.focus();
  }
  const field = (
    <div className="position-saved-field">
      <label>
        <span>{p('savedSearches')}</span>
        <select
          aria-label={p('chooseSearch')}
          value={chosen}
          disabled={busy}
          onChange={(event) => choose(event.target.value)}
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
        ref={menuButton}
        icon="more"
        label={p('savedSearchActions')}
        compact
        type="button"
        popoverTarget={menuId}
        disabled={busy}
      />
      <div
        id={menuId}
        ref={menu}
        popover="auto"
        className="position-saved-menu"
        onToggle={(event) => setMenuOpen(event.newState === 'open')}
        onKeyDown={(event) => {
          if (event.key !== 'Escape') return;
          event.preventDefault();
          event.stopPropagation();
          closeMenu();
        }}
      >
        <ActionButton
          icon="save"
          label={p('saveCurrent')}
          type="button"
          disabled={busy}
          onClick={() => {
            closeMenu();
            setError(undefined);
            setInvalid(false);
            setSaving(true);
          }}
        />
        <ActionButton
          icon="delete"
          label={p('deleteSearch')}
          type="button"
          disabled={!active || busy}
          onClick={() => {
            closeMenu();
            void remove();
          }}
        />
        <ActionButton
          icon="refresh"
          label={p('reload')}
          type="button"
          disabled={busy}
          onClick={() => {
            closeMenu();
            setReload((value) => value + 1);
          }}
        />
      </div>
    </div>
  );
  const problem = (
    <>
      {invalid && <p role="alert">{p('invalidFilter')}</p>}
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
    </>
  );
  const messages = [
    active?.database_missing && (
      <p key="missing" role="alert">
        {p('databaseMissing')}
      </p>
    ),
    changed && (
      <p key="dirty" className="position-unsaved">
        {p('dirty')}
      </p>
    ),
    !saving && (invalid || !!error) && <div key="problem">{problem}</div>,
    busy && !saving && (
      <p key="busy" role="status">
        {p('saving')}
      </p>
    ),
    notice && (
      <p key="notice" role="status" className="position-undo-notice">
        {p(notice)}
        {notice === 'deleted' && deleted && (
          <button type="button" onClick={() => void restore()}>
            {p('undo')}
          </button>
        )}
      </p>
    ),
  ].filter(Boolean);
  const primary = active ? 'update' : 'new';
  const status = (
    <>
      {messages.length > 0 && (
        <div className="position-saved-status">{messages}</div>
      )}
      {saving && (
        <DeckDialog
          title={p('saveSearchTitle')}
          close={() => setSaving(false)}
          busy={busy}
          className="position-save-search-dialog"
          actions={
            <>
              <ActionButton
                icon="save"
                label={p('saveNew')}
                type="button"
                className={primary === 'new' ? 'primary' : undefined}
                disabled={!name.trim() || busy}
                onClick={() => void save(false)}
              />
              {active && (
                <ActionButton
                  icon="save"
                  label={p('updateSearch')}
                  type="button"
                  className="primary"
                  disabled={!name.trim() || busy}
                  onClick={() => void save(true)}
                />
              )}
            </>
          }
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (name.trim() && !busy) void save(primary === 'update');
            }}
          >
            <label>
              <span>{p('searchName')}</span>
              <input
                autoFocus
                value={name}
                maxLength={120}
                disabled={busy}
                onChange={(event) => setName(event.target.value)}
              />
            </label>
          </form>
          {problem}
          {busy && <p role="status">{p('saving')}</p>}
        </DeckDialog>
      )}
    </>
  );
  return { field, status };
}

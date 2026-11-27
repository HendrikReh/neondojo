import { useEffect, useId, useRef, useState } from 'react';
import { useIsMounted } from '../../shared/hooks/useIsMounted';
import { usePopoverPlacement } from '../../shared/hooks/usePopoverPlacement';
import { useI18n } from '../../shared/i18n/i18n';
import ActionButton from '../../shared/ui/ActionButton';
import { DeckDialog } from '../study/flashcards/DeckLibraryDialogs';
import {
  deletePositionFilter,
  loadPositionFilters,
  positionDefinition,
  samePositionDefinition,
  savePositionFilter,
  type PositionFilterDraft,
  type SavedPositionFilter,
} from './position-browser-api';
import { usePositionText } from './position-browser-messages';

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
  const mounted = useIsMounted(),
    generation = useRef(0);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );
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
      .catch((failure) => {
        if (!abort.signal.aborted) setError(failure);
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
      if (!mounted.current || version !== generation.current) return;
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
    } catch (failure) {
      if (mounted.current && version === generation.current) setError(failure);
    } finally {
      if (mounted.current && version === generation.current) setBusy(false);
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
      if (!mounted.current || version !== generation.current) return;
      setItems((previous) => previous.filter((other) => other.id !== item.id));
      setActive(undefined);
      setChosen('');
      setName('');
      setNotice('deleted');
      setDeleted(item);
      setReload((value) => value + 1);
    } catch (failure) {
      if (mounted.current && version === generation.current) setError(failure);
    } finally {
      if (mounted.current && version === generation.current) setBusy(false);
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
      if (!mounted.current || version !== generation.current) return;
      setItems((previous) =>
        [...previous.filter((other) => other.id !== saved.id), saved].sort(
          (a, b) => a.name.localeCompare(b.name),
        ),
      );
      setDeleted(undefined);
      setChosen(saved.id);
      setNotice('saved');
      setReload((value) => value + 1);
    } catch (failure) {
      if (mounted.current && version === generation.current) setError(failure);
    } finally {
      if (mounted.current && version === generation.current) setBusy(false);
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

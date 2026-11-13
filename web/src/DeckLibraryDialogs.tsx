import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import ActionButton from './ActionButton';
import {
  addDeckCards,
  deleteCards,
  isDeckConflict,
  loadDuplicates,
  moveDeckCards,
  removeDeckCards,
  resolveDuplicate,
  type DuplicateCard,
  type DuplicateDeck,
  type VersionedDeck,
} from './deck-management-api';
import type { components } from './generated-api';
import { useDeckLibraryText } from './deck-library-messages';
import { useDeckManagementText } from './deck-management-messages';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';
import './deck-management.css';

function DeckDialog({
  title,
  children,
  close,
  busy = false,
  wide = false,
  focusCancel = false,
}: {
  title: string;
  children: ReactNode;
  close: () => void;
  busy?: boolean;
  wide?: boolean;
  focusCancel?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const heading = useId();
  const t = useDeckManagementText();
  useEffect(() => {
    const element = dialog.current;
    const trigger = document.activeElement;
    element?.showModal();
    if (focusCancel) cancel.current?.focus();
    return () => {
      element?.close();
      if (trigger instanceof HTMLElement && trigger.isConnected)
        trigger.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className={`deck-management-dialog${wide ? ' deck-library-dialog' : ''}`}
      aria-labelledby={heading}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) close();
      }}
    >
      <h2 id={heading}>{title}</h2>
      {children}
      <footer>
        <ActionButton
          ref={cancel}
          icon="close"
          label={t('cancel')}
          disabled={busy}
          onClick={close}
        >
          {t('cancel')}
        </ActionButton>
      </footer>
    </dialog>
  );
}

export function DeckLibraryDialog({
  decks,
  create,
  close,
  open,
  manage,
}: {
  decks: VersionedDeck[];
  create: ReactNode;
  close: () => void;
  open: (id: string) => void;
  manage: (mode: 'rename' | 'delete', deck: VersionedDeck) => void;
}) {
  const t = useDeckLibraryText();
  const actions = useDeckManagementText();
  const learning = useLearningText();
  const { locale } = useI18n();
  const [query, setQuery] = useState('');
  const needle = query.trim().toLocaleLowerCase(locale);
  const matching = decks
    .filter((deck) => deck.name.toLocaleLowerCase(locale).includes(needle))
    .sort((a, b) => a.name.localeCompare(b.name, locale));
  return (
    <DeckDialog title={t('decks')} close={close} wide>
      {create}
      <label className="deck-management-name">
        {t('find')}
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <ul className="deck-library-list">
        {matching.map((deck) => (
          <li key={deck.id}>
            <div>
              <strong>{deck.name}</strong>
              <br />
              {learning('cards', { count: deck.card_count })}
            </div>
            <div className="learning-controls">
              <ActionButton
                icon="open"
                label={t('open')}
                onClick={() => open(deck.id)}
              >
                {t('open')}
              </ActionButton>
              <ActionButton
                icon="edit"
                label={actions('rename')}
                onClick={() => manage('rename', deck)}
              >
                {actions('rename')}
              </ActionButton>
              <ActionButton
                icon="delete"
                label={actions('delete')}
                onClick={() => manage('delete', deck)}
              >
                {actions('delete')}
              </ActionButton>
            </div>
          </li>
        ))}
      </ul>
      {!matching.length && <p role="status">{t('noDecks')}</p>}
    </DeckDialog>
  );
}

type DeleteCard = components['schemas']['DeleteCard'];
export type CardBatchAction =
  | { mode: 'add'; source?: VersionedDeck; ids: string[] }
  | { mode: 'move' | 'remove'; source: VersionedDeck; ids: string[] }
  | { mode: 'delete'; cards: DeleteCard[] };

const batchLabel = {
  add: 'addCards',
  move: 'move',
  remove: 'removeCards',
  delete: 'deleteCards',
} as const;

export function CardBatchDialog({
  action,
  decks,
  close,
  completed,
  refresh,
}: {
  action: CardBatchAction;
  decks: VersionedDeck[];
  close: () => void;
  completed: (message: string) => void;
  refresh: () => void;
}) {
  const t = useDeckLibraryText();
  const { formatError, formatNumber, locale, t: ui } = useI18n();
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const [stale, setStale] = useState(false);
  const needsTarget = action.mode === 'add' || action.mode === 'move';
  const options = needsTarget
    ? decks
        .filter((deck) => deck.id !== action.source?.id)
        .sort((a, b) => a.name.localeCompare(b.name, locale))
    : [];
  const target = options.find((deck) => deck.id === destination);
  const count = formatNumber(
    action.mode === 'delete' ? action.cards.length : action.ids.length,
  );
  const title = t(batchLabel[action.mode], { count });
  async function submit() {
    if ((needsTarget && !target) || busy || stale) return;
    setBusy(true);
    setError(undefined);
    try {
      switch (action.mode) {
        case 'add': {
          if (!target) return;
          const result = await addDeckCards(target, action.ids);
          completed(
            t('addedCards', {
              count: formatNumber(result.added),
              existing: formatNumber(result.already_in_target),
              name: target.name,
            }),
          );
          break;
        }
        case 'move': {
          if (!target) return;
          const result = await moveDeckCards(action.source, target, action.ids);
          completed(
            t('moved', {
              count: formatNumber(result.moved),
              name: target.name,
            }),
          );
          break;
        }
        case 'remove':
          await removeDeckCards(action.source, action.ids);
          completed(t('removedCards', { count, name: action.source.name }));
          break;
        case 'delete':
          await deleteCards(action.cards);
          completed(t('deletedCards', { count }));
          break;
      }
    } catch (failure) {
      setStale(isDeckConflict(failure));
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <DeckDialog
      title={title}
      close={close}
      busy={busy}
      focusCancel={!needsTarget}
    >
      {action.mode === 'add' && <p>{t('addCardsHelp', { count })}</p>}
      {action.mode === 'move' && (
        <p>{t('moveHelp', { count, name: action.source.name })}</p>
      )}
      {action.mode === 'remove' && (
        <p>{t('removeCardsHelp', { count, name: action.source.name })}</p>
      )}
      {action.mode === 'delete' && <p>{t('deleteCardsHelp', { count })}</p>}
      {needsTarget && (
        <label className="deck-management-name">
          {t('target')}
          <select
            value={destination}
            disabled={busy}
            onChange={(event) => setDestination(event.target.value)}
          >
            <option value="">—</option>
            {options.map((deck) => (
              <option key={deck.id} value={deck.id}>
                {deck.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {!!error && <p role="alert">{stale ? t('stale') : formatError(error)}</p>}
      {stale ? (
        <ActionButton
          icon="refresh"
          label={ui('games.retry')}
          onClick={refresh}
        >
          {ui('games.retry')}
        </ActionButton>
      ) : (
        <ActionButton
          icon={
            action.mode === 'add'
              ? 'add'
              : action.mode === 'move'
                ? 'move'
                : 'delete'
          }
          label={title}
          className={
            action.mode === 'delete' ? 'deck-management-delete' : undefined
          }
          disabled={(needsTarget && !target) || busy}
          onClick={() => void submit()}
        >
          {title}
        </ActionButton>
      )}
    </DeckDialog>
  );
}

type DuplicateAction =
  | { card: DuplicateCard; deck: DuplicateDeck }
  | { card: DuplicateCard; keep: DuplicateCard };

export function DuplicateCardsDialog({
  close,
  onChanged,
}: {
  close: () => void;
  onChanged: () => void;
}) {
  const t = useDeckLibraryText();
  const learning = useLearningText();
  const management = useDeckManagementText();
  const { formatError, formatNumber, t: ui } = useI18n();
  const [items, setItems] = useState<DuplicateCard[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [writing, setWriting] = useState(false);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<unknown>();
  const [stale, setStale] = useState(false);
  const [action, setAction] = useState<DuplicateAction>();
  const [keepers, setKeepers] = useState<Record<string, string>>({});
  const active = useRef<AbortController | undefined>(undefined);
  async function load(after: string | null, controller: AbortController) {
    setLoading(true);
    setError(undefined);
    try {
      const page = await loadDuplicates(after, controller.signal);
      if (controller.signal.aborted) return;
      setItems((old) => (after ? [...old, ...page.items] : page.items));
      setCursor(page.next_cursor);
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure);
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }
  useEffect(() => {
    const controller = new AbortController();
    active.current = controller;
    void load(null, controller);
    return () => active.current?.abort();
  }, [revision]);
  function reload() {
    setAction(undefined);
    setKeepers({});
    setStale(false);
    setItems([]);
    setCursor(null);
    setRevision((value) => value + 1);
  }
  async function confirm() {
    if (!action || writing) return;
    setWriting(true);
    setError(undefined);
    try {
      if ('deck' in action)
        await removeDeckCards(action.deck, [action.card.card.id]);
      else await resolveDuplicate(action.keep, action.card);
      reload();
      onChanged();
    } catch (failure) {
      setStale(isDeckConflict(failure));
      setError(failure);
    } finally {
      setWriting(false);
    }
  }
  function label(entry: DuplicateCard) {
    const question =
      entry.card.kind === 'checker'
        ? 'questionChecker'
        : entry.card.kind === 'cube_offer'
          ? 'questionOffer'
          : 'questionResponse';
    return `${learning(question)} · ${entry.card.players[entry.card.position.player]} · ${learning('reviewCount', { count: entry.card.review_count })} · ${entry.decks.map((d) => d.name).join(', ') || '—'}`;
  }
  return (
    <DeckDialog title={t('duplicates')} close={close} busy={writing} wide>
      {loading && <p role="status">{learning('loading')}</p>}
      {!!error && <p role="alert">{stale ? t('stale') : formatError(error)}</p>}
      {!loading && (stale || (error && !items.length)) ? (
        <ActionButton icon="refresh" label={ui('games.retry')} onClick={reload}>
          {ui('games.retry')}
        </ActionButton>
      ) : null}
      {!loading && !error && !items.length && <p role="status">{t('empty')}</p>}
      {action && !stale && (
        <div
          className="duplicate-confirmation"
          role="group"
          aria-label={
            'deck' in action
              ? t('remove', { name: action.deck.name })
              : t('delete')
          }
        >
          <p>
            {'deck' in action
              ? t('removeHelp', { name: action.deck.name })
              : t('deleteHelp', {
                  count: formatNumber(action.card.card.review_count),
                })}
          </p>
          {'keep' in action && (
            <p>
              <strong>{t('keep')}</strong> · {label(action.keep)} ·{' '}
              {action.keep.card.id.slice(0, 8)}
            </p>
          )}
          <div className="learning-controls">
            <ActionButton
              icon="delete"
              label={
                'deck' in action
                  ? t('remove', { name: action.deck.name })
                  : t('delete')
              }
              disabled={writing}
              onClick={() => void confirm()}
            >
              {'deck' in action
                ? t('remove', { name: action.deck.name })
                : t('delete')}
            </ActionButton>
            <ActionButton
              icon="close"
              label={management('cancel')}
              disabled={writing}
              onClick={() => setAction(undefined)}
            >
              {management('cancel')}
            </ActionButton>
          </div>
        </div>
      )}
      <ul className="deck-library-list">
        {items.map((entry) => {
          const { card, duplicate_key: key } = entry;
          const peers = key
            ? items.filter(
                (item) =>
                  item.duplicate_key === key && item.card.id !== card.id,
              )
            : [];
          const keep = peers.find((peer) => peer.card.id === keepers[card.id]);
          return (
            <li key={card.id}>
              <div className="duplicate-entry">
                <strong>
                  {card.position.position_id} / {card.position.gnubg_match_id}
                </strong>
                <span>
                  {label(entry)}
                  {key && ` · ${card.id.slice(0, 8)}`}
                </span>
                {entry.decks.length > 1 && (
                  <div className="learning-controls">
                    <span>{t('shared')}</span>
                    {entry.decks.map((deck) => (
                      <ActionButton
                        key={deck.id}
                        icon="delete"
                        label={t('remove', { name: deck.name })}
                        disabled={!!action || stale || loading}
                        onClick={() => setAction({ card: entry, deck })}
                      >
                        {t('remove', { name: deck.name })}
                      </ActionButton>
                    ))}
                  </div>
                )}
                {key && (
                  <div className="learning-controls">
                    <span>
                      {t('exact', { count: formatNumber(entry.copies) })}
                    </span>
                    {!!peers.length && (
                      <>
                        <label>
                          {t('keep')}
                          <select
                            value={keep?.card.id ?? ''}
                            disabled={!!action || stale || loading}
                            onChange={(event) =>
                              setKeepers((old) => ({
                                ...old,
                                [card.id]: event.target.value,
                              }))
                            }
                          >
                            <option value="">—</option>
                            {peers.map((peer) => (
                              <option key={peer.card.id} value={peer.card.id}>
                                {label(peer)} · {peer.card.id.slice(0, 8)}
                              </option>
                            ))}
                          </select>
                        </label>
                        <ActionButton
                          icon="delete"
                          label={t('delete')}
                          disabled={!keep || !!action || stale || loading}
                          onClick={() =>
                            keep && setAction({ card: entry, keep })
                          }
                        >
                          {t('delete')}
                        </ActionButton>
                      </>
                    )}
                  </div>
                )}
              </div>
            </li>
          );
        })}
      </ul>
      {cursor && (
        <ActionButton
          icon="add"
          label={learning('more')}
          disabled={loading || !!action || stale}
          onClick={() => {
            const controller = new AbortController();
            active.current = controller;
            void load(cursor, controller);
          }}
        >
          {learning('more')}
        </ActionButton>
      )}
    </DeckDialog>
  );
}

import ActionButton from './ActionButton';
import { useEffect, useRef, useState, useId } from 'react';
import { request } from './api';
import { useI18n } from './i18n';
import { AppError } from './i18n-core';
import type { Deck } from './learning-api';
import {
  transferPositions,
  type PositionBrowserHit,
  type PositionTransferResult,
} from './position-browser-api';
import { usePositionText } from './position-browser-messages';

export function PositionTransferOutcomes({
  result,
  names,
  onOpen,
}: {
  result: PositionTransferResult;
  names: Record<string, string>;
  onOpen: (id: string) => void;
}) {
  const p = usePositionText();
  return (
    <div className="position-transfer-results" role="status">
      <h3>{p('outcome')}</h3>
      <ul>
        {result.items.map((item, index) => (
          <li
            key={`${item.position_id}-${index}`}
            data-transfer-status={item.status}
          >
            <span>{names[item.position_id] ?? item.position_id}</span>
            <strong>{p(item.status)}</strong>
            {item.status === 'analysis_required' && (
              <ActionButton
                icon="analysis"
                label={p('openAnalysis')}
                onClick={() => onOpen(item.position_id)}
              />
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
export default function PositionBrowserDecks({
  revision,
  selected,
  hits,
  onChanged,
  onStudy,
  onOpen,
  onBusyChange,
}: {
  revision: number;
  selected: ReadonlySet<string>;
  hits: PositionBrowserHit[];
  onChanged: () => void;
  onStudy: (id: string) => void;
  onOpen: (id: string) => void;
  onBusyChange: (busy: boolean) => void;
}) {
  const deckCountId = useId();
  const p = usePositionText(),
    { locale, formatError } = useI18n();
  const [decks, setDecks] = useState<Deck[]>([]),
    [deckId, setDeckId] = useState(''),
    [name, setName] = useState(''),
    [busy, setBusy] = useState(false),
    [exporting, setExporting] = useState(false),
    [error, setError] = useState<unknown>(),
    [result, setResult] = useState<PositionTransferResult>(),
    [resultNames, setResultNames] = useState<Record<string, string>>({});
  const live = useRef(true),
    generation = useRef(0),
    changed = useRef(onChanged),
    busyChanged = useRef(onBusyChange);
  changed.current = onChanged;
  busyChanged.current = onBusyChange;
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      generation.current++;
      busyChanged.current(false);
    };
  }, []);
  useEffect(() => {
    const abort = new AbortController();
    void request<Deck[]>('/decks', { signal: abort.signal })
      .then((items) => {
        if (abort.signal.aborted) return;
        setDecks(items);
        setDeckId((previous) =>
          items.some((deck) => deck.id === previous)
            ? previous
            : (items[0]?.id ?? ''),
        );
      })
      .catch((error) => {
        if (!abort.signal.aborted) setError(error);
      });
    return () => abort.abort();
  }, [revision]);
  function setWorking(value: boolean) {
    setBusy(value);
    busyChanged.current(value);
  }
  async function create() {
    const version = ++generation.current;
    setWorking(true);
    setError(undefined);
    try {
      const deck = await request<Deck>('/decks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: name.trim() }),
      });
      if (!live.current || version !== generation.current) return;
      setDecks((previous) => [
        ...previous.filter((item) => item.id !== deck.id),
        deck,
      ]);
      setDeckId(deck.id);
      setName('');
      changed.current();
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setWorking(false);
    }
  }
  async function add() {
    if (!deckId || !selected.size) return;
    const version = ++generation.current,
      id = deckId,
      ids = [...selected];
    const names = Object.fromEntries(
      hits
        .filter((hit) => selected.has(hit.position.id))
        .map((hit) => [
          hit.position.id,
          `${hit.database_name} · ${hit.display_name} · ${p('game', { number: hit.position.game_no })} · ${p('ply', { number: hit.position.ply })}`,
        ]),
    );
    setWorking(true);
    setError(undefined);
    setResult(undefined);
    try {
      const value = await transferPositions(id, ids);
      if (!live.current || version !== generation.current) return;
      setResult(value);
      setResultNames(names);
      setDecks((previous) =>
        previous.map((deck) =>
          deck.id === id ? { ...deck, card_count: value.total } : deck,
        ),
      );
      changed.current();
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setWorking(false);
    }
  }
  async function download() {
    if (!deckId) return;
    setExporting(true);
    setError(undefined);
    try {
      const response = await fetch(
        `/api/decks/${deckId}/export.apkg?locale=${locale}`,
      );
      if (!response.ok) {
        let diagnostic = await response.text();
        try {
          diagnostic = JSON.parse(diagnostic).error ?? diagnostic;
        } catch {
          /* Plain HTTP error. */
        }
        throw new AppError(
          'error.request',
          { status: String(response.status) },
          diagnostic,
        );
      }
      const blob = await response.blob();
      if (!live.current) return;
      const url = URL.createObjectURL(blob),
        anchor = document.createElement('a');
      anchor.href = url;
      anchor.download =
        /filename="([^"]+)"/.exec(
          response.headers.get('Content-Disposition') ?? '',
        )?.[1] ?? 'bgtutor.apkg';
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      if (live.current) setError(error);
    } finally {
      if (live.current) setExporting(false);
    }
  }
  const chosenDeck = decks.find((deck) => deck.id === deckId),
    hasCards = (chosenDeck?.card_count ?? 0) > 0;
  return (
    <section className="position-deck-panel" aria-label={p('deck')}>
      <h2>{p('deck')}</h2>
      <p>{p('transferHint')}</p>
      <div className="position-deck-controls">
        <label>
          <span>{p('chooseDeck')}</span>
          <select
            aria-label={p('chooseDeck')}
            aria-describedby={deckCountId}
            value={deckId}
            disabled={busy || exporting}
            onChange={(event) => setDeckId(event.target.value)}
          >
            {!decks.length && <option value="">—</option>}
            {decks.map((deck) => (
              <option key={deck.id} value={deck.id}>
                {deck.name}
              </option>
            ))}
          </select>
          <small id={deckCountId}>
            {p('deckCount', { count: chosenDeck?.card_count ?? 0 })}
          </small>
        </label>
        <ActionButton
          icon="add"
          label={p(busy ? 'transfer' : 'addSelected')}
          compact
          className="primary"
          disabled={!deckId || !selected.size || busy || exporting}
          onClick={() => void add()}
        />
        <ActionButton
          icon="cards"
          label={p('study')}
          disabled={!hasCards || busy || exporting}
          onClick={() => onStudy(deckId)}
        />
      </div>
      <form
        className="position-create-deck"
        onSubmit={(event) => {
          event.preventDefault();
          void create();
        }}
      >
        <label>
          <span>{p('deckName')}</span>
          <input
            value={name}
            maxLength={120}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <ActionButton
          icon="add"
          label={p('createDeck')}
          disabled={!name.trim() || busy}
        />
      </form>
      <div className="position-deck-export">
        <ActionButton
          icon="export"
          label={p(exporting ? 'exporting' : 'exportDeck')}
          compact
          disabled={!hasCards || busy || exporting}
          onClick={() => void download()}
        />
        <small>{p('exportHint')}</small>
      </div>
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
      {result && (
        <PositionTransferOutcomes
          result={result}
          names={resultNames}
          onOpen={onOpen}
        />
      )}
    </section>
  );
}

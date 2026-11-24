import ActionButton from '../../shared/ui/ActionButton';
import { useEffect, useRef, useState, useId, type ReactNode } from 'react';
import { useLatest } from '../../shared/hooks/useLatest';
import { request } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import { DeckDialog } from '../study/flashcards/DeckLibraryDialogs';
import type { Deck } from '../study/flashcards/learning-api';
import {
  transferPositions,
  loadPositionAnalysisReadiness,
  type PositionAnalysisReadiness,
  type PositionBrowserHit,
  type PositionTransferResult,
} from './position-browser-api';
import { usePositionText } from './position-browser-messages';

export function PositionTransferOutcomes({
  result,
  names,
  onOpen,
  action,
}: {
  result: PositionTransferResult;
  names: Record<string, string>;
  onOpen: (id: string) => void;
  /** Follow-up for the deck that received the positions. */
  action?: ReactNode;
}) {
  const p = usePositionText();
  return (
    <div className="position-transfer-results" role="status">
      <div className="position-transfer-heading">
        <h3>{p('outcome')}</h3>
        {action}
      </div>
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
  transferOpen,
  onCloseTransfer,
  onChanged,
  onStudy,
  onOpen,
  onBusyChange,
}: {
  revision: number;
  selected: ReadonlySet<string>;
  hits: PositionBrowserHit[];
  transferOpen: boolean;
  onCloseTransfer: () => void;
  onChanged: () => void;
  onStudy: (id: string) => void;
  onOpen: (id: string) => void;
  onBusyChange: (busy: boolean) => void;
}) {
  const deckCountId = useId();
  const p = usePositionText(),
    { formatError } = useI18n();
  const [decks, setDecks] = useState<Deck[]>([]),
    [deckId, setDeckId] = useState(''),
    [name, setName] = useState(''),
    [busy, setBusy] = useState(false),
    [deckError, setDeckError] = useState<unknown>(),
    [error, setError] = useState<unknown>(),
    [readiness, setReadiness] = useState<{
      selection: ReadonlySet<string>;
      summary?: PositionAnalysisReadiness;
      error?: unknown;
    }>(),
    [readinessAttempt, setReadinessAttempt] = useState(0),
    [result, setResult] = useState<PositionTransferResult>(),
    [resultDeck, setResultDeck] = useState<Deck>(),
    [resultNames, setResultNames] = useState<Record<string, string>>({});
  const live = useRef(true),
    generation = useRef(0),
    changed = useLatest(onChanged),
    busyChanged = useLatest(onBusyChange);
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
        setDeckError(undefined);
        setDecks(items);
        setDeckId((previous) =>
          items.some((deck) => deck.id === previous)
            ? previous
            : (items[0]?.id ?? ''),
        );
      })
      .catch((error) => {
        if (!abort.signal.aborted) setDeckError(error);
      });
    return () => abort.abort();
  }, [revision]);
  useEffect(() => {
    if (!transferOpen || !selected.size) {
      setReadiness(undefined);
      return;
    }
    const abort = new AbortController();
    setReadiness(undefined);
    void loadPositionAnalysisReadiness([...selected], abort.signal)
      .then((summary) => {
        if (!abort.signal.aborted)
          setReadiness({ selection: selected, summary });
      })
      .catch((error) => {
        if (!abort.signal.aborted) setReadiness({ selection: selected, error });
      });
    return () => abort.abort();
  }, [transferOpen, selected, readinessAttempt]);
  const check = readiness?.selection === selected ? readiness : undefined;
  const summary = check?.summary;
  const ready = summary
    ? selected.size - summary.analysis_required - summary.unavailable
    : 0;
  const analysisTarget = summary?.first_analysis_required;
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
    if (!deckId || ready <= 0 || busy) return;
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
      setResultDeck(decks.find((deck) => deck.id === id));
      setResultNames(names);
      setDecks((previous) =>
        previous.map((deck) =>
          deck.id === id ? { ...deck, card_count: value.total } : deck,
        ),
      );
      onCloseTransfer();
      changed.current();
    } catch (error) {
      if (live.current && version === generation.current) setError(error);
    } finally {
      if (live.current && version === generation.current) setWorking(false);
    }
  }
  const chosenDeck = decks.find((deck) => deck.id === deckId);
  const deckPicker = (countId: string) => (
    <label>
      <span>{p('chooseDeck')}</span>
      <select
        aria-label={p('chooseDeck')}
        aria-describedby={countId}
        value={deckId}
        disabled={busy}
        onChange={(event) => setDeckId(event.target.value)}
      >
        {!decks.length && <option value="">—</option>}
        {decks.map((deck) => (
          <option key={deck.id} value={deck.id}>
            {deck.name}
          </option>
        ))}
      </select>
      <small id={countId}>
        {p('deckCount', { count: chosenDeck?.card_count ?? 0 })}
      </small>
    </label>
  );
  return (
    <>
      {result && (
        <PositionTransferOutcomes
          result={result}
          names={resultNames}
          onOpen={onOpen}
          action={
            resultDeck &&
            result.total > 0 && (
              <ActionButton
                icon="cards"
                label={p('study')}
                tooltip={resultDeck.name}
                disabled={busy}
                onClick={() => onStudy(resultDeck.id)}
              />
            )
          }
        />
      )}
      {!!deckError && !transferOpen && (
        <p role="alert" className="error">
          {formatError(deckError)}
        </p>
      )}
      {transferOpen && selected.size > 0 && (
        <DeckDialog
          title={p('addToDeck')}
          close={() => {
            setError(undefined);
            onCloseTransfer();
          }}
          busy={busy}
          className="position-transfer-dialog"
          actions={
            ready > 0 ? (
              <ActionButton
                icon="add"
                label={
                  busy
                    ? p('transfer')
                    : p(ready === 1 ? 'addOne' : 'addCount', { count: ready })
                }
                type="button"
                className="primary"
                disabled={!deckId || busy}
                onClick={() => void add()}
              />
            ) : (
              analysisTarget && (
                <ActionButton
                  icon="analysis"
                  label={p('openAnalysis')}
                  type="button"
                  className="primary"
                  onClick={() => {
                    onCloseTransfer();
                    onOpen(analysisTarget);
                  }}
                />
              )
            )
          }
        >
          {check?.error ? (
            <p role="alert">
              {formatError(check.error)}{' '}
              <button
                type="button"
                onClick={() => {
                  setReadiness(undefined);
                  setReadinessAttempt((attempt) => attempt + 1);
                }}
              >
                {p('retrySimilar')}
              </button>
            </p>
          ) : (
            <div role="status" className="position-transfer-readiness">
              {summary ? (
                <>
                  {ready === 0 && <p>{p('noneReady')}</p>}
                  {!!summary.analysis_required && (
                    <p className="position-analysis-needed">
                      {p('needsLocalAnalysis', {
                        count: summary.analysis_required,
                        total: selected.size,
                      })}
                    </p>
                  )}
                  {!!summary.unavailable && (
                    <p>
                      {p('unavailablePositions', {
                        count: summary.unavailable,
                      })}
                    </p>
                  )}
                </>
              ) : (
                <p>{p('checkingAnalysis')}</p>
              )}
            </div>
          )}
          {ready > 0 && (
            <>
              <div className="position-deck-controls">
                {deckPicker(`${deckCountId}-transfer`)}
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
                  type="submit"
                  icon="add"
                  label={p('createDeck')}
                  disabled={!name.trim() || busy}
                />
              </form>
              {(!!deckError || !!error) && (
                <p role="alert" className="error">
                  {formatError(deckError ?? error)}
                </p>
              )}
            </>
          )}
        </DeckDialog>
      )}
    </>
  );
}

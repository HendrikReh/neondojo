import ActionButton from '../../shared/ui/ActionButton';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useIsMounted } from '../../shared/hooks/useIsMounted';
import { useLatest } from '../../shared/hooks/useLatest';
import { useI18n } from '../../shared/i18n/i18n';
import CreateDeckForm from '../study/flashcards/CreateDeckForm';
import { DeckDialog } from '../study/flashcards/DeckLibraryDialogs';
import DeckSelectField from '../study/flashcards/DeckSelectField';
import type { Deck } from '../study/flashcards/learning-api';
import { savedDeck, useDecks } from '../study/flashcards/useDecks';
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
  const p = usePositionText(),
    { formatError } = useI18n();
  const {
    decks,
    error: deckError,
    addDeck,
    replaceDeck,
  } = useDecks(revision);
  const [deckId, setDeckId] = useState(savedDeck),
    [busy, setBusy] = useState(false),
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
  const mounted = useIsMounted(),
    generation = useRef(0),
    changed = useLatest(onChanged),
    busyChanged = useLatest(onBusyChange);
  const chosenDeck = decks.find((deck) => deck.id === deckId) ?? decks[0];

  useEffect(
    () => () => {
      generation.current++;
      busyChanged.current(false);
    },
    [busyChanged],
  );

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
      .catch((failure) => {
        if (!abort.signal.aborted)
          setReadiness({ selection: selected, error: failure });
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

  async function add() {
    if (!chosenDeck || ready <= 0 || busy) return;
    const version = ++generation.current,
      targetDeck = chosenDeck,
      id = targetDeck.id,
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
      if (!mounted.current || version !== generation.current) return;
      setResult(value);
      setResultDeck(targetDeck);
      setResultNames(names);
      replaceDeck({ ...targetDeck, card_count: value.total });
      onCloseTransfer();
      changed.current();
    } catch (failure) {
      if (mounted.current && version === generation.current) setError(failure);
    } finally {
      if (mounted.current && version === generation.current) setWorking(false);
    }
  }

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
                disabled={!chosenDeck || busy}
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
                <DeckSelectField
                  decks={decks}
                  value={chosenDeck?.id ?? ''}
                  onChange={setDeckId}
                  label={p('chooseDeck')}
                  disabled={busy}
                  countText={p('deckCount', {
                    count: chosenDeck?.card_count ?? 0,
                  })}
                />
              </div>
              <CreateDeckForm
                className="position-create-deck"
                label={p('deckName')}
                buttonLabel={p('createDeck')}
                showLabel
                disabled={busy}
                onBusyChange={setWorking}
                onCreated={(deck) => {
                  addDeck(deck);
                  setDeckId(deck.id);
                  changed.current();
                }}
              />
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

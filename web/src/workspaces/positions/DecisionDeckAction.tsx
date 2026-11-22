import { useEffect, useId, useRef, useState } from 'react';
import type { components } from '../../generated-api';
import { request } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import ActionButton from '../../shared/ui/ActionButton';
import CreateDeckForm from '../study/flashcards/CreateDeckForm';
import { DeckDialog } from '../study/flashcards/DeckLibraryDialogs';
import { savedDeck, useDecks } from '../study/flashcards/useDecks';
import { usePositionText } from './position-browser-messages';

type AddCardsResult = components['schemas']['AddCardsResult'];
export type DecisionDeckOutcome = {
  positionId: string;
  deck: string;
  result: AddCardsResult;
};

/**
 * Adds one analysed checker decision to a deck. The card uses the analysis
 * the decision was found in, so an unsaved position can become a card.
 */
export default function DecisionDeckAction({
  positionId,
  analysisId,
  revision,
  onAdded,
  onDeckCreated,
}: {
  positionId: string;
  /** The catalog entry of the decision's analysis, when it is known. */
  analysisId?: string;
  revision: number;
  onAdded: (outcome: DecisionDeckOutcome) => void;
  onDeckCreated: () => void;
}) {
  const p = usePositionText();
  const [open, setOpen] = useState(false);
  return (
    <>
      <ActionButton
        type="button"
        icon="cards"
        label={p('addToDeck')}
        compact
        aria-haspopup="dialog"
        onClick={() => setOpen(true)}
      />
      {open && (
        <DecisionDeckDialog
          positionId={positionId}
          analysisId={analysisId}
          revision={revision}
          onClose={() => setOpen(false)}
          onAdded={(deck, result) => {
            setOpen(false);
            onAdded({ positionId, deck, result });
          }}
          onDeckCreated={onDeckCreated}
        />
      )}
    </>
  );
}

/** The deck that received the decision and its new card count. */
export function DecisionDeckStatus({
  outcome,
}: {
  outcome: DecisionDeckOutcome;
}) {
  const p = usePositionText();
  return (
    <p className="decision-deck-outcome" role="status">
      {p(outcome.result.added > 0 ? 'added' : 'updated')} · {outcome.deck} ·{' '}
      {p('deckCount', { count: outcome.result.total })}
    </p>
  );
}

function DecisionDeckDialog({
  positionId,
  analysisId,
  revision,
  onClose,
  onAdded,
  onDeckCreated,
}: {
  positionId: string;
  analysisId?: string;
  revision: number;
  onClose: () => void;
  onAdded: (deck: string, result: AddCardsResult) => void;
  onDeckCreated: () => void;
}) {
  const p = usePositionText();
  const { formatError } = useI18n();
  const countId = useId();
  const { decks, error: deckError, addDeck } = useDecks(revision);
  const [deckId, setDeckId] = useState(savedDeck);
  const chosen = decks.find((deck) => deck.id === deckId) ?? decks[0];
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  async function add() {
    if (!chosen || busy) return;
    setBusy(true);
    setError(undefined);
    try {
      const result = await request<AddCardsResult>(
        `/decks/${encodeURIComponent(chosen.id)}/cards`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            position_id: positionId,
            analysis_id: analysisId,
          } satisfies components['schemas']['AddCards']),
        },
      );
      if (live.current) onAdded(chosen.name, result);
    } catch (error) {
      if (live.current) setError(error);
    } finally {
      if (live.current) setBusy(false);
    }
  }
  return (
    <DeckDialog
      title={p('addToDeck')}
      close={onClose}
      busy={busy}
      className="position-transfer-dialog"
      actions={
        <ActionButton
          type="button"
          icon="add"
          className="primary"
          label={busy ? p('transfer') : p('addOne')}
          disabled={!chosen || busy}
          onClick={() => void add()}
        />
      }
    >
      <div className="position-deck-controls">
        <label>
          <span>{p('chooseDeck')}</span>
          <select
            aria-label={p('chooseDeck')}
            aria-describedby={countId}
            value={chosen?.id ?? ''}
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
            {p('deckCount', { count: chosen?.card_count ?? 0 })}
          </small>
        </label>
      </div>
      <CreateDeckForm
        onCreated={(deck) => {
          addDeck(deck);
          setDeckId(deck.id);
          onDeckCreated();
        }}
      />
      {!!(error || deckError) && (
        <p role="alert" className="error">
          {formatError(error || deckError)}
        </p>
      )}
    </DeckDialog>
  );
}

import { useEffect, useId, useRef, useState } from 'react';
import type { components } from '../../generated-api';
import { request } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import ActionButton from '../../shared/ui/ActionButton';
import CreateDeckForm from '../study/flashcards/CreateDeckForm';
import { DeckDialog } from '../study/flashcards/DeckLibraryDialogs';
import { savedDeck, useDecks } from '../study/flashcards/useDecks';
import { useDecisionSearchText } from './decision-search-messages';
import { usePositionText } from './position-browser-messages';

type AddDecisionsResult = components['schemas']['AddDecisionsResult'];
type DecisionAddStatus = components['schemas']['DecisionAddStatus'];
type MistakeReference = components['schemas']['MistakeReference'];
export type DecisionDeckOutcome = {
  /** Identifies what was added: one decision or the shown page. */
  key: string;
  deck: string;
  result: AddDecisionsResult;
};

/**
 * Adds analysed decisions, checker plays and cube decisions alike, to a deck.
 * Each card uses the analysis its decision was found in, so a decision
 * without a saved position can become a card.
 */
export default function DecisionDeckAction({
  references,
  outcomeKey,
  label,
  tooltip,
  compact = true,
  revision,
  onAdded,
  onDeckCreated,
}: {
  /** 1 to 200 decisions in the order shown. */
  references: MistakeReference[];
  outcomeKey: string;
  label?: string;
  tooltip?: string;
  compact?: boolean;
  revision: number;
  onAdded: (outcome: DecisionDeckOutcome) => void;
  onDeckCreated: () => void;
}) {
  const p = usePositionText();
  const [open, setOpen] = useState(false);
  const text = label ?? p('addToDeck');
  return (
    <>
      <ActionButton
        type="button"
        icon="cards"
        label={text}
        tooltip={tooltip}
        compact={compact}
        aria-haspopup="dialog"
        disabled={references.length === 0}
        onClick={() => setOpen(true)}
      >
        {compact ? undefined : text}
      </ActionButton>
      {open && (
        <DecisionDeckDialog
          title={text}
          references={references}
          revision={revision}
          onClose={() => setOpen(false)}
          onAdded={(deck, result) => {
            setOpen(false);
            onAdded({ key: outcomeKey, deck, result });
          }}
          onDeckCreated={onDeckCreated}
        />
      )}
    </>
  );
}

/** What the deck received and its new card count. */
export function DecisionDeckStatus({
  outcome,
}: {
  outcome: DecisionDeckOutcome;
}) {
  const p = usePositionText();
  const t = useDecisionSearchText();
  const { items, total } = outcome.result;
  const count = (status: DecisionAddStatus) =>
    items.filter((item) => item.status === status).length;
  const summary =
    items.length === 1
      ? single(items[0].status, p, t)
      : t('deckOutcome', {
          added: count('added'),
          updated: count('updated'),
          duplicate: count('duplicate'),
          unavailable: count('unavailable') + count('not_found'),
        });
  return (
    <p className="decision-deck-outcome" role="status">
      {summary} · {outcome.deck} · {p('deckCount', { count: total })}
    </p>
  );
}

function single(
  status: DecisionAddStatus,
  p: ReturnType<typeof usePositionText>,
  t: ReturnType<typeof useDecisionSearchText>,
) {
  switch (status) {
    case 'added':
      return p('added');
    case 'updated':
      return p('updated');
    case 'duplicate':
      return p('duplicate');
    case 'not_found':
      return p('not_found');
    case 'unavailable':
      return t('cardUnavailable');
  }
}

function DecisionDeckDialog({
  title,
  references,
  revision,
  onClose,
  onAdded,
  onDeckCreated,
}: {
  title: string;
  references: MistakeReference[];
  revision: number;
  onClose: () => void;
  onAdded: (deck: string, result: AddDecisionsResult) => void;
  onDeckCreated: () => void;
}) {
  const p = usePositionText();
  const t = useDecisionSearchText();
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
      const result = await request<AddDecisionsResult>(
        `/decks/${encodeURIComponent(chosen.id)}/decisions`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            decisions: references,
          } satisfies components['schemas']['AddDecisions']),
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
      title={title}
      close={onClose}
      busy={busy}
      className="position-transfer-dialog"
      actions={
        <ActionButton
          type="button"
          icon="add"
          className="primary"
          label={
            busy
              ? t('addingDecisions')
              : t('addDecisions', { count: references.length })
          }
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

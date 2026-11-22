import { useEffect, useRef, useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { request } from '../../../shared/http/api';
import { useI18n } from '../../../shared/i18n/i18n';
import type { Deck } from './learning-api';
import { useLearningText } from './learning-messages';

export default function CreateDeckForm({
  onCreated,
}: {
  onCreated: (deck: Deck) => void;
}) {
  const t = useLearningText();
  const { formatError } = useI18n();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  async function create() {
    setBusy(true);
    setError(null);
    try {
      const deck = await request<Deck>('/decks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: name.trim() }),
      });
      if (live.current) {
        setName('');
        onCreated(deck);
      }
    } catch (error) {
      if (live.current) setError(error);
    } finally {
      if (live.current) setBusy(false);
    }
  }
  return (
    <form
      className="create-deck"
      onSubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <input
        aria-label={t('deckName')}
        placeholder={t('deckName')}
        maxLength={120}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <ActionButton
        icon="add"
        label={t('create')}
        disabled={busy || !name.trim()}
      >
        {t('create')}
      </ActionButton>
      {!!error && <p role="alert">{formatError(error)}</p>}
    </form>
  );
}

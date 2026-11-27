import { useState } from 'react';
import { useIsMounted } from '../../../shared/hooks/useIsMounted';
import ActionButton from '../../../shared/ui/ActionButton';
import { request } from '../../../shared/http/api';
import { useI18n } from '../../../shared/i18n/i18n';
import type { Deck } from './learning-api';
import { useLearningText } from './learning-messages';

export default function CreateDeckForm({
  id,
  hidden,
  disabled = false,
  className = 'create-deck',
  label,
  buttonLabel,
  showLabel = false,
  onBusyChange,
  onCreated,
}: {
  id?: string;
  hidden?: boolean;
  disabled?: boolean;
  className?: string;
  label?: string;
  buttonLabel?: string;
  showLabel?: boolean;
  onBusyChange?: (busy: boolean) => void;
  onCreated: (deck: Deck) => void;
}) {
  const t = useLearningText();
  const { formatError } = useI18n();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const mounted = useIsMounted();
  const inputLabel = label ?? t('deckName');
  const submitLabel = buttonLabel ?? t('create');
  async function create() {
    setBusy(true);
    onBusyChange?.(true);
    setError(null);
    try {
      const deck = await request<Deck>('/decks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: name.trim() }),
      });
      if (mounted.current) {
        setName('');
        onCreated(deck);
      }
    } catch (error) {
      if (mounted.current) setError(error);
    } finally {
      if (mounted.current) {
        setBusy(false);
        onBusyChange?.(false);
      }
    }
  }
  return (
    <form
      id={id}
      hidden={hidden}
      className={className}
      onSubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      {showLabel ? (
        <label>
          <span>{inputLabel}</span>
          <input
            maxLength={120}
            disabled={busy || disabled}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
      ) : (
        <input
          aria-label={inputLabel}
          placeholder={inputLabel}
          maxLength={120}
          disabled={busy || disabled}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      )}
      <ActionButton
        type="submit"
        icon="add"
        label={submitLabel}
        disabled={busy || disabled || !name.trim()}
      >
        {showLabel ? undefined : submitLabel}
      </ActionButton>
      {!!error && <p role="alert">{formatError(error)}</p>}
    </form>
  );
}

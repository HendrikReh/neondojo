import { useEffect, useId, useState } from 'react';
import ActionButton from './shared/ui/ActionButton';
import { request } from './shared/api/api';
import { useCardPracticeText } from './card-practice-messages';
import { useI18n } from './shared/i18n/i18n';
import type { CardTags, EditCardTags } from './learning-api';
import { SearchFilterInput } from './shared/ui/SearchFilterForm';

export default function CardTagEditor({
  cards,
  tags,
  onSaved,
}: {
  cards: EditCardTags['cards'];
  tags?: readonly string[];
  onSaved?: (cards: CardTags[]) => void;
}) {
  const t = useCardPracticeText();
  const { formatError } = useI18n();
  const [add, setAdd] = useState('');
  const [remove, setRemove] = useState('');
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [saved, setSaved] = useState<CardTags[]>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const list = useId();
  useEffect(() => {
    const abort = new AbortController();
    if (!cards.length) return;
    void request<string[]>('/cards/tags', { signal: abort.signal })
      .then((tags) => {
        if (!abort.signal.aborted) setSuggestions(tags);
      })
      .catch(() => {});
    return () => abort.abort();
  }, [cards.length]);

  async function save() {
    setBusy(true);
    setError(undefined);
    try {
      const versions = cards.map((card) => {
        const edited = saved?.find(({ id }) => id === card.id);
        return {
          id: card.id,
          tags_version: Math.max(card.tags_version, edited?.tags_version ?? 0),
        };
      });
      const result = await request<CardTags[]>('/cards/tags', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          cards: versions,
          add: add.trim() ? [add] : [],
          remove: remove.trim() ? [remove] : [],
        } satisfies EditCardTags),
      });
      setSaved(result);
      setAdd('');
      setRemove('');
      onSaved?.(result);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  if (!cards.length) return null;
  const current =
    cards.length === 1
      ? (saved?.find(({ id }) => id === cards[0].id)?.tags ?? tags)
      : tags;
  return (
    <details>
      <summary>{t('cardTags')}</summary>
      {!!current?.length && <p>{current.join(' · ')}</p>}
      <div className="learning-controls">
        <SearchFilterInput
          label={t('addTag')}
          value={add}
          onChange={setAdd}
          list={list}
          maxLength={64}
          disabled={busy}
        />
        <SearchFilterInput
          label={t('removeTag')}
          value={remove}
          onChange={setRemove}
          list={list}
          maxLength={64}
          disabled={busy}
        />
        <datalist id={list}>
          {suggestions.map((tag) => (
            <option key={tag} value={tag} />
          ))}
        </datalist>
        <ActionButton
          type="button"
          icon="confirm"
          label={t('applyTags')}
          disabled={busy || (!add.trim() && !remove.trim())}
          onClick={() => void save()}
        >
          {t('applyTags')}
        </ActionButton>
      </div>
      {!!error && <p role="alert">{formatError(error)}</p>}
    </details>
  );
}

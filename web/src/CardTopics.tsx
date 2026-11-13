import { useEffect, useState } from 'react';
import ActionButton from './ActionButton';
import { request } from './api';
import { useCardPracticeText } from './card-practice-messages';
import {
  ALL_CARDS,
  CARDS_WITHOUT_DECK,
  emptyCardFilters,
  isCollectionScope,
  type CardFilters,
} from './card-search-api';
import { useI18n } from './i18n';
import type { CardQuery, CardTopic } from './learning-api';
import { SearchFilterInput, SearchFilterSelect } from './SearchFilterForm';

export default function CardTopics({
  filters,
  deck,
  onSelect,
}: {
  filters: CardFilters;
  deck: string;
  onSelect: (deck: string, filters: CardFilters) => void;
}) {
  const t = useCardPracticeText();
  const { formatError } = useI18n();
  const [topics, setTopics] = useState<CardTopic[]>([]);
  const [selected, setSelected] = useState('');
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  useEffect(() => {
    const abort = new AbortController();
    void request<CardTopic[]>('/study/card-topics', { signal: abort.signal })
      .then(setTopics)
      .catch((failure) => {
        if (!abort.signal.aborted) setError(failure);
      });
    return () => abort.abort();
  }, []);
  function select(id: string) {
    setSelected(id);
    const topic = topics.find((value) => value.id === id);
    if (!topic) return;
    const f = topic.filter;
    onSelect(f.without_deck ? CARDS_WITHOUT_DECK : (f.deck_id ?? ALL_CARDS), {
      ...emptyCardFilters(),
      q: f.q ?? '',
      player: f.player ?? '',
      tag: f.tag ?? '',
      topics: f.topics ?? undefined,
      game_plan: f.game_plan ?? undefined,
      comment: f.comment ?? '',
      position_id: f.position_id ?? '',
      kind: f.kind ?? '',
      status: f.status ?? '',
    });
  }
  async function save() {
    setBusy(true);
    setError(undefined);
    try {
      const filter: CardQuery = {
        ...filters,
        game_plan: filters.game_plan || undefined,
        kind: filters.kind || undefined,
        status: filters.status || undefined,
        deck_id: isCollectionScope(deck) ? undefined : deck,
        without_deck: deck === CARDS_WITHOUT_DECK || undefined,
      };
      const topic = await request<CardTopic>('/study/card-topics', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, filter }),
      });
      setTopics((items) =>
        [...items, topic].sort((a, b) => a.name.localeCompare(b.name)),
      );
      setSelected(topic.id);
      setName('');
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    setBusy(true);
    setError(undefined);
    try {
      await request<void>(`/study/card-topics/${selected}`, {
        method: 'DELETE',
      });
      setTopics((items) => items.filter((topic) => topic.id !== selected));
      setSelected('');
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <details>
      <summary>{t('topics')}</summary>
      <div className="learning-controls">
        <SearchFilterSelect
          label={t('topics')}
          value={selected}
          onChange={select}
        >
          <option value="">—</option>
          {topics.map((topic) => (
            <option value={topic.id} key={topic.id}>
              {topic.name}
            </option>
          ))}
        </SearchFilterSelect>
        <ActionButton
          type="button"
          icon="delete"
          label={t('deleteTopic')}
          compact
          disabled={busy || !selected}
          onClick={() => void remove()}
        />
        <SearchFilterInput
          label={t('topicName')}
          value={name}
          onChange={setName}
          maxLength={120}
        />
        <ActionButton
          type="button"
          icon="confirm"
          label={t('saveTopic')}
          disabled={busy || !name.trim()}
          onClick={() => void save()}
        >
          {t('saveTopic')}
        </ActionButton>
      </div>
      {!!error && <p role="alert">{formatError(error)}</p>}
    </details>
  );
}

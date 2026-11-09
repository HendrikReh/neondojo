import { useState, type KeyboardEvent } from 'react';
import Board, { type Theme } from './Board';
import { matchContext } from './boardMath';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';
import type { CardQuestion, Deck } from './learning-api';
import type { CardBrowseContext } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { usePositionText } from './position-browser-messages';
import './learning-card-browser.css';

export default function LearningCardBrowser({
  cards,
  theme,
  hasMore,
  moreLoading,
  onMore,
  visible = true,
  total,
  loading = false,
  decks = [],
}: {
  cards: readonly (CardQuestion & { browse?: CardBrowseContext })[];
  total?: number;
  loading?: boolean;
  decks?: readonly Pick<Deck, 'id' | 'name'>[];
  theme: Theme;
  hasMore: boolean;
  moreLoading: boolean;
  onMore: () => void;
  visible?: boolean;
}) {
  const searchT = useCardSearchText(),
    p = usePositionText();
  const deckName = (deck: { id: string; name: string }) =>
    decks.find((item) => item.id === deck.id)?.name ?? deck.name;
  const t = useLearningText(),
    { t: uiT, formatNumber } = useI18n();
  const [selectedId, setSelectedId] = useState<string>();
  const selected = cards.find((card) => card.id === selectedId) ?? cards[0];
  const context = selected
    ? matchContext(selected.position.gnubg_match_id)
    : undefined;
  const question = (card: CardQuestion) =>
    t(
      card.kind === 'checker'
        ? 'questionChecker'
        : card.kind === 'cube_response'
          ? 'questionResponse'
          : 'questionOffer',
    );
  function rowKey(event: KeyboardEvent<HTMLTableRowElement>, index: number) {
    if (event.target !== event.currentTarget) return;
    let next = index;
    if (event.key === 'ArrowDown') next++;
    else if (event.key === 'ArrowUp') next--;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = cards.length - 1;
    else if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    next = Math.max(0, Math.min(cards.length - 1, next));
    setSelectedId(cards[next].id);
    document.getElementById(`learning-card-${cards[next].id}`)?.focus();
  }
  return (
    <div className="learning-card-browser">
      <div className="learning-card-list">
        <div className="learning-results-heading">
          <h3>{p('results')}</h3>
          <span role="status">
            {searchT('found', { count: total ?? cards.length })}
          </span>
        </div>
        <div className="learning-card-table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">{t('question')}</th>
                <th scope="col">{p('player')}</th>
                <th scope="col">{p('opponent')}</th>
                <th scope="col">{p('decks')}</th>
                <th scope="col">{t('reviews')}</th>
              </tr>
            </thead>
            <tbody>
              {cards.map((card, index) => (
                <tr
                  key={card.id}
                  id={`learning-card-${card.id}`}
                  data-card-id={card.id}
                  title={[
                    question(card),
                    card.players.join(' · '),
                    card.position.position_id,
                    card.browse?.decks.map(deckName).join(', '),
                    card.browse?.tags.join(', '),
                    card.browse?.comment_excerpt,
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                  tabIndex={0}
                  aria-selected={selected?.id === card.id}
                  className={
                    selected?.id === card.id
                      ? 'learning-card-selected'
                      : undefined
                  }
                  onClick={(event) => {
                    event.currentTarget.focus();
                    setSelectedId(card.id);
                  }}
                  onKeyDown={(event) => rowKey(event, index)}
                >
                  <td>
                    <button
                      type="button"
                      className="learning-card-select"
                      aria-label={t('previewCard', { number: index + 1 })}
                      onClick={() => setSelectedId(card.id)}
                    >
                      {question(card)}
                    </button>
                    <small>{card.position.position_id}</small>
                  </td>
                  <td>{card.players[card.position.player]}</td>
                  <td>{card.players[1 - card.position.player]}</td>
                  <td>
                    {card.browse?.decks.length
                      ? card.browse.decks.map(deckName).join(', ')
                      : p('noDecks')}
                  </td>
                  <td>{formatNumber(card.review_count)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!loading && !cards.length && (
          <p className="learning-no-results" role="status">
            {searchT('noResults')}
          </p>
        )}
        {hasMore && (
          <button disabled={moreLoading} onClick={onMore}>
            {t(moreLoading ? 'loading' : 'more')}
          </button>
        )}
      </div>
      <aside
        className="learning-card-preview"
        aria-label={t('cardPreview')}
        data-card-id={selected?.id}
      >
        <h3>{t('cardPreview')}</h3>
        {!selected || !context ? (
          <p>{t('previewHint')}</p>
        ) : (
          <>
            <h4>{question(selected)}</h4>
            <p className="study-context">
              {context.length
                ? uiT('app.matchLength', { count: context.length })
                : `${uiT('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}{' '}
              ·{' '}
              {uiT('app.toMove', {
                player: selected.players[selected.position.player],
              })}
            </p>
            {selected.browse?.decks.length ? (
              <p className="learning-preview-decks">
                {p('decks')}: {selected.browse.decks.map(deckName).join(', ')}
              </p>
            ) : null}
            {selected.kind === 'cube_response' && (
              <p>{t('offeredCube', { value: context.cube * 2 })}</p>
            )}
            {visible && (
              <Board
                position={selected.position}
                white={selected.players[0]}
                black={selected.players[1]}
                theme={theme}
                previewLabel={t('cardPreview')}
              />
            )}
            {!!selected.browse?.tags.length && (
              <div className="learning-preview-tags">
                {selected.browse.tags.map((tag) => (
                  <span key={tag}>{tag}</span>
                ))}
              </div>
            )}
            {!!selected.browse?.comment_excerpt && (
              <p className="learning-preview-comment">
                {selected.browse.comment_excerpt}
              </p>
            )}
          </>
        )}
      </aside>
    </div>
  );
}

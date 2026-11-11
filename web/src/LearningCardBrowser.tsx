import { useState, type ReactNode } from 'react';
import type { Theme } from './themes';
import SearchPagination, { useSearchResultPage } from './SearchPagination';
import {
  SearchResultList,
  SearchResultNotes,
  SearchResultPlayers,
  SearchResultRow,
  SearchResultTable,
  SearchResultTags,
} from './SearchResultList';
import {
  SearchPreviewBoard,
  SearchPreviewDetails,
  SearchResultPreview,
} from './SearchResultPreview';
import { matchContext } from './boardMath';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';
import type { CardQuestion, Deck } from './learning-api';
import { CARD_PAGE_SIZE, type CardBrowseContext } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { usePositionText } from './position-browser-messages';

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
  children,
}: {
  cards: readonly (CardQuestion & { browse?: CardBrowseContext })[];
  total?: number;
  loading?: boolean;
  decks?: readonly Pick<Deck, 'id' | 'name'>[];
  theme: Theme;
  hasMore: boolean;
  moreLoading: boolean;
  onMore: () => Promise<boolean>;
  visible?: boolean;
  children?: ReactNode;
}) {
  const searchT = useCardSearchText(),
    p = usePositionText();
  const deckName = (deck: { id: string; name: string }) =>
    decks.find((item) => item.id === deck.id)?.name ?? deck.name;
  const t = useLearningText(),
    { t: uiT, formatNumber } = useI18n();
  const [selectedId, setSelectedId] = useState<string>();
  const [pageIndex, setPageIndex] = useState(0);
  const { page, lastPage, start, rows } = useSearchResultPage(
    cards,
    CARD_PAGE_SIZE,
    pageIndex,
  );
  const ids = rows.map((card) => card.id);
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
  const preview = (
    <SearchResultPreview
      className="learning-card-preview"
      title={t('cardPreview')}
      hint={t('previewHint')}
      data-card-id={selected?.id}
    >
      {selected && context && (
        <>
          <h4>{question(selected)}</h4>
          <p className="position-preview-context">
            {context.length
              ? uiT('app.matchLength', { count: context.length })
              : `${uiT('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}
          </p>
          <SearchPreviewBoard
            position={selected.position}
            white={selected.players[0]}
            black={selected.players[1]}
            theme={theme}
            label={t('cardPreview')}
            visible={visible}
          >
            {selected.kind === 'cube_response' && (
              <p>{t('offeredCube', { value: context.cube * 2 })}</p>
            )}
          </SearchPreviewBoard>
          <SearchPreviewDetails
            positionId={selected.position.position_id}
            decks={selected.browse?.decks.map(deckName).join(', ')}
          />
          {!!selected.browse?.tags.length && (
            <div className="annotation-tags">
              <SearchResultTags tags={selected.browse.tags} />
            </div>
          )}
          {!!selected.browse?.comment_excerpt && (
            <p className="learning-preview-comment">
              {selected.browse.comment_excerpt}
            </p>
          )}
        </>
      )}
    </SearchResultPreview>
  );
  return (
    <SearchResultList
      className="learning-card-browser"
      listClassName="learning-card-list"
      busy={loading || moreLoading}
      count={
        <span role="status">
          {searchT('found', { count: total ?? cards.length })}
        </span>
      }
      preview={preview}
    >
      {loading && <p role="status">{p('loading')}</p>}
      {rows.length > 0 && (
        <SearchResultTable
          key={page}
          scrollClassName="learning-card-table-scroll"
        >
          <thead>
            <tr>
              <th scope="col">{t('question')}</th>
              <th scope="col">{p('player')}</th>
              <th scope="col">{p('opponent')}</th>
              <th scope="col">{p('decks')}</th>
              <th scope="col">{p('notes')}</th>
              <th scope="col">{t('reviews')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((card, index) => (
              <SearchResultRow
                key={card.id}
                ids={ids}
                index={index}
                idPrefix="learning-card-"
                data-card-id={card.id}
                text={[
                  question(card),
                  card.players.join(' · '),
                  card.position.position_id,
                  card.browse?.decks.map(deckName).join(', '),
                  card.browse?.tags.join(', '),
                  card.browse?.comment_excerpt,
                ]
                  .filter(Boolean)
                  .join(' · ')}
                aria-selected={selected?.id === card.id}
                active={selected?.id === card.id}
                selectedClassName="learning-card-selected"
                jumpToEdges
                onPreview={setSelectedId}
              >
                <td>
                  <button
                    type="button"
                    className="position-preview-link"
                    aria-label={t('previewCard', {
                      number: start + index + 1,
                    })}
                    onClick={() => setSelectedId(card.id)}
                  >
                    <strong>{question(card)}</strong>
                    <small className="position-match-name">
                      {card.position.position_id}
                    </small>
                  </button>
                </td>
                <SearchResultPlayers
                  player={card.players[card.position.player]}
                  opponent={card.players[1 - card.position.player]}
                />
                <td>
                  {card.browse?.decks.length
                    ? card.browse.decks.map(deckName).join(', ')
                    : p('noDecks')}
                </td>
                <td>
                  <SearchResultNotes
                    tags={card.browse?.tags}
                    comment={card.browse?.comment_excerpt}
                  />
                </td>
                <td>{formatNumber(card.review_count)}</td>
              </SearchResultRow>
            ))}
          </tbody>
        </SearchResultTable>
      )}
      {!loading && !cards.length && (
        <p className="position-empty" role="status">
          {searchT('noResults')}
        </p>
      )}
      <SearchPagination
        page={page}
        lastPage={lastPage}
        busy={loading || moreLoading}
        onPageChange={setPageIndex}
        onMore={hasMore ? onMore : undefined}
        moreLabel={t(moreLoading ? 'loading' : 'more')}
      />
      {children}
    </SearchResultList>
  );
}

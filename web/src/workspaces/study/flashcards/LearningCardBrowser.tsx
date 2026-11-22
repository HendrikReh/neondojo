import AnkiCardFace from './AnkiCardFace';
import { useState, type CSSProperties, type ReactNode } from 'react';
import type { Theme } from '../../../shared/board/themes';
import SearchPagination from '../../../shared/ui/SearchPagination';
import { lastSearchPage } from '../../../shared/hooks/useSearchPages';
import {
  SearchResultList,
  SearchResultNotes,
  SearchResultPlayers,
  SearchResultRow,
  SearchResultTable,
  SearchResultTags,
} from '../../../shared/ui/SearchResultList';
import {
  SearchPreviewBoard,
  SearchPreviewDetails,
  SearchResultPreview,
} from '../../../shared/ui/SearchResultPreview';
import { matchContext } from '../../../shared/board/boardMath';
import { useI18n } from '../../../shared/i18n/i18n';
import { useLearningText } from './learning-messages';
import type { CardQuestion, Deck } from './learning-api';
import {
  CARD_PAGE_SIZE,
  CARD_SELECTION_LIMIT,
  type CardBrowseContext,
} from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { usePositionText } from '../../positions/position-browser-messages';
import { useDeckLibraryText } from './deck-library-messages';
import { useCardPreviewResize } from './useCardPreviewResize';

export default function LearningCardBrowser({
  cards,
  theme,
  page = 1,
  onPageChange,
  visible = true,
  total,
  loading = false,
  decks = [],
  children,
  selection,
  actions,
}: {
  cards: readonly (CardQuestion & { browse?: CardBrowseContext })[];
  total?: number;
  loading?: boolean;
  decks?: readonly Pick<Deck, 'id' | 'name'>[];
  theme: Theme;
  page?: number;
  onPageChange: (page: number) => void;
  visible?: boolean;
  children?: ReactNode;
  selection?: {
    ids: ReadonlySet<string>;
    toggle: (ids: string[], checked: boolean) => void;
    disabled: boolean;
  };
  actions?: ReactNode;
}) {
  const deckText = useDeckLibraryText();
  const resize = useCardPreviewResize(visible);
  const searchT = useCardSearchText(),
    p = usePositionText();
  const deckName = (deck: { id: string; name: string }) =>
    decks.find((item) => item.id === deck.id)?.name ?? deck.name;
  const t = useLearningText(),
    { t: uiT, formatNumber } = useI18n();
  const [previewCard, setPreviewCard] = useState<(typeof cards)[number]>();
  const start = (page - 1) * CARD_PAGE_SIZE;
  const ids = cards.map((card) => card.id);
  const selectedOnPage = ids.filter((id) => selection?.ids.has(id)).length;
  const selected =
    cards.find((card) => card.id === previewCard?.id) ??
    (cards.length ? previewCard : undefined) ??
    cards[0];
  const context = selected?.position
    ? matchContext(selected.position.gnubg_match_id)
    : undefined;
  const question = (card: CardQuestion) =>
    card.kind === 'anki'
      ? card.prompt
      : t(
          card.kind === 'checker'
            ? 'questionChecker'
            : card.kind === 'cube_response'
              ? 'questionResponse'
              : 'questionOffer',
        );
  const summaries = cards.map((card) => {
    if (!card.position) return searchT('anki');
    const { scores, length, jacoby, cube, owner, crawford } = matchContext(
      card.position.gnubg_match_id,
    );
    const dice = card.position.dice;
    return [
      length
        ? `${scores.map((score) => formatNumber(score)).join(':')} / ${formatNumber(length)}`
        : uiT('common.moneyGame'),
      !length && jacoby ? 'Jacoby' : '',
      crawford ? p('crawford') : '',
      card.kind === 'checker' &&
      dice.length === 2 &&
      dice.every((die) => die >= 1 && die <= 6)
        ? `${p('dice')} ${dice.join('–')}`
        : '',
      card.kind === 'cube_response'
        ? t('offeredCube', { value: cube * 2 })
        : owner === null
          ? `${p('cubeShort')} ${formatNumber(cube)}`
          : uiT('board.cube', {
              value: cube,
              owner: card.players[owner],
            }),
    ]
      .filter(Boolean)
      .join(' · ');
  });
  const preview = (
    <SearchResultPreview
      className="learning-card-preview"
      id={resize.previewId}
      title={t('cardPreview')}
      hint={t('previewHint')}
      data-card-id={selected?.id}
    >
      {visible !== false && selected && selected.kind === 'anki' && (
        <AnkiCardFace id={selected.id} title={selected.prompt} />
      )}
      {selected?.position && context && (
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
          {!!selected.browse?.card_tags?.length && (
            <div className="annotation-tags">
              <SearchResultTags tags={selected.browse.card_tags} />
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
      ref={resize.layout}
      style={{ '--card-preview-width': `${resize.width}px` } as CSSProperties}
      divider={resize.divider}
      listClassName="learning-card-list"
      busy={loading}
      count={
        <span role="status">
          {searchT('found', { count: total ?? cards.length })}
        </span>
      }
      preview={preview}
    >
      {actions}
      {loading && <p role="status">{p('loading')}</p>}
      {cards.length > 0 && (
        <SearchResultTable
          key={page}
          className={
            selection ? 'position-table learning-card-selectable' : undefined
          }
          scrollClassName="learning-card-table-scroll"
        >
          <thead>
            <tr>
              {selection && (
                <th scope="col" className="learning-card-select-column">
                  <input
                    type="checkbox"
                    aria-label={p('selectPage')}
                    checked={selectedOnPage === ids.length}
                    ref={(input) => {
                      if (input)
                        input.indeterminate =
                          selectedOnPage > 0 && selectedOnPage < ids.length;
                    }}
                    disabled={
                      selection.disabled ||
                      (selectedOnPage === 0 &&
                        selection.ids.size >= CARD_SELECTION_LIMIT)
                    }
                    onChange={() =>
                      selection.toggle(
                        ids,
                        selectedOnPage < ids.length &&
                          selection.ids.size < CARD_SELECTION_LIMIT,
                      )
                    }
                  />
                </th>
              )}
              <th scope="col">{t('question')}</th>
              <th scope="col">{p('player')}</th>
              <th scope="col">{p('opponent')}</th>
              <th scope="col">{p('decks')}</th>
              <th scope="col">{p('notes')}</th>
              <th scope="col">{t('reviews')}</th>
            </tr>
          </thead>
          <tbody>
            {cards.map((card, index) => (
              <SearchResultRow
                key={card.id}
                ids={ids}
                index={index}
                idPrefix="learning-card-"
                data-card-id={card.id}
                text={[
                  question(card),
                  summaries[index],
                  card.players.join(' · '),
                  card.position?.position_id,
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
                onPreview={(id) =>
                  setPreviewCard(cards.find((card) => card.id === id))
                }
              >
                {selection && (
                  <td
                    className="learning-card-select-column"
                    onClick={(event) => event.stopPropagation()}
                  >
                    <input
                      type="checkbox"
                      aria-label={deckText('select', {
                        number: formatNumber(start + index + 1),
                      })}
                      checked={selection.ids.has(card.id)}
                      disabled={
                        selection.disabled ||
                        (!selection.ids.has(card.id) &&
                          selection.ids.size >= CARD_SELECTION_LIMIT)
                      }
                      onChange={(event) =>
                        selection.toggle([card.id], event.target.checked)
                      }
                    />
                  </td>
                )}
                <td>
                  <button
                    type="button"
                    className="position-preview-link"
                    aria-label={t('previewCard', {
                      number: start + index + 1,
                    })}
                    onClick={() => setPreviewCard(card)}
                  >
                    <strong>{question(card)}</strong>
                    <small className="position-match-name">
                      {summaries[index]}
                    </small>
                  </button>
                </td>
                {card.position ? (
                  <SearchResultPlayers
                    player={card.players[card.position.player]}
                    opponent={card.players[1 - card.position.player]}
                  />
                ) : (
                  <SearchResultPlayers player="—" opponent="—" />
                )}
                <td>
                  {card.browse?.decks.length
                    ? card.browse.decks.map(deckName).join(', ')
                    : p('noDecks')}
                </td>
                <td>
                  <SearchResultNotes
                    tags={[
                      ...new Set([
                        ...(card.browse?.card_tags ?? []),
                        ...(card.browse?.tags ?? []),
                      ]),
                    ]}
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
        lastPage={lastSearchPage(total ?? cards.length, CARD_PAGE_SIZE)}
        busy={loading}
        onPageChange={onPageChange}
      />
      {children}
    </SearchResultList>
  );
}

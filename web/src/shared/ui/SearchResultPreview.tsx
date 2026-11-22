import { Children, type ComponentPropsWithoutRef, type ReactNode } from 'react';
import type { Position } from '../api/api';
import Board from '../board/Board';
import type { Theme } from '../board/themes';
import { usePositionText } from '../../workspaces/positions/position-browser-messages';

export function SearchResultPreview({
  title,
  hint,
  hideTitle,
  className,
  children,
  ...panel
}: ComponentPropsWithoutRef<'aside'> & {
  title: string;
  hint: string;
  /** The panel keeps its accessible name without a visible heading. */
  hideTitle?: boolean;
}) {
  return (
    <aside
      {...panel}
      className={
        className ? `${className} position-preview` : 'position-preview'
      }
      aria-label={title}
    >
      {!hideTitle && <h2>{title}</h2>}
      {Children.toArray(children).length ? children : <p>{hint}</p>}
    </aside>
  );
}

export function SearchPreviewBoard({
  position,
  white,
  black,
  theme,
  label,
  visible,
  children,
}: {
  position: Position;
  white: string;
  black: string;
  theme: Theme;
  label: string;
  visible: boolean;
  children?: ReactNode;
}) {
  const t = usePositionText();
  const whiteOnRoll = position.player === 0;
  return (
    <>
      <p className="position-preview-players">
        <strong>
          {t('player')}: {whiteOnRoll ? white : black}
        </strong>
        <span>
          {t('opponent')}: {whiteOnRoll ? black : white}
        </span>
      </p>
      {children}
      {visible && (
        <Board
          position={position}
          white={white}
          black={black}
          theme={theme}
          previewLabel={label}
        />
      )}
    </>
  );
}

export function SearchPreviewDetails({
  positionId,
  decks,
  hideDecks,
  children,
}: {
  positionId: string;
  decks?: string;
  /** For callers that show the decks outside the details. */
  hideDecks?: boolean;
  children?: ReactNode;
}) {
  const t = usePositionText();
  return (
    <>
      <dl className="position-context-details">
        <div>
          <dt>{t('checkerId')}</dt>
          <dd>{positionId}</dd>
        </div>
        {children}
      </dl>
      {!hideDecks && <SearchPreviewDecks decks={decks} />}
    </>
  );
}

export function SearchPreviewDecks({ decks }: { decks?: string }) {
  const t = usePositionText();
  return (
    <p className="position-preview-decks">
      <strong>{t('decks')}:</strong> {decks || t('noDecks')}
    </p>
  );
}

import { Children, type ComponentPropsWithoutRef, type ReactNode } from 'react';
import type { Position } from './api';
import Board from './Board';
import type { Theme } from './themes';
import { usePositionText } from './position-browser-messages';

export function SearchResultPreview({
  title,
  hint,
  className,
  children,
  ...panel
}: ComponentPropsWithoutRef<'aside'> & { title: string; hint: string }) {
  return (
    <aside
      {...panel}
      className={
        className ? `${className} position-preview` : 'position-preview'
      }
      aria-label={title}
    >
      <h2>{title}</h2>
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
  children,
}: {
  positionId: string;
  decks?: string;
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
      <p className="position-preview-decks">
        <strong>{t('decks')}:</strong> {decks || t('noDecks')}
      </p>
    </>
  );
}

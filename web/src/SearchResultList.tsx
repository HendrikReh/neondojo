import type { ComponentPropsWithRef, KeyboardEvent, ReactNode } from 'react';
import Tooltip from './Tooltip';
import { usePositionText } from './position-browser-messages';

export function SearchResultList({
  className,
  listClassName,
  busy,
  title,
  count,
  hideHeading,
  preview,
  divider,
  children,
  ...layout
}: {
  className?: string;
  listClassName?: string;
  busy?: boolean;
  /** Replaces the generic results heading. */
  title?: ReactNode;
  count?: ReactNode;
  /** Keeps the results heading for assistive technology only. */
  hideHeading?: boolean;
  preview: ReactNode;
  divider?: ReactNode;
  children: ReactNode;
} & ComponentPropsWithRef<'div'>) {
  const t = usePositionText();
  return (
    <div
      {...layout}
      className={
        className
          ? `${className} position-browser-layout`
          : 'position-browser-layout'
      }
    >
      <div
        className={
          listClassName
            ? `${listClassName} position-browser-main`
            : 'position-browser-main'
        }
        aria-busy={busy}
      >
        {hideHeading ? (
          <h2 className="visually-hidden">{title ?? t('results')}</h2>
        ) : (
          <div className="position-result-heading">
            <h2>{title ?? t('results')}</h2>
            {count}
          </div>
        )}
        {children}
      </div>
      {divider}
      {preview}
    </div>
  );
}

export function SearchResultTable({
  scrollClassName,
  className = 'position-table',
  ...table
}: ComponentPropsWithRef<'table'> & { scrollClassName?: string }) {
  const t = usePositionText();
  return (
    <div
      className={
        scrollClassName
          ? `${scrollClassName} position-table-scroll`
          : 'position-table-scroll'
      }
    >
      <table className={className} aria-label={t('results')} {...table} />
    </div>
  );
}

export function SearchResultRow({
  ids,
  index,
  idPrefix,
  text,
  active,
  selectedClassName,
  onPreview,
  onOpen,
  jumpToEdges = false,
  ...row
}: ComponentPropsWithRef<'tr'> & {
  ids: readonly string[];
  index: number;
  idPrefix: string;
  text?: string;
  active: boolean;
  selectedClassName?: string;
  onPreview: (id: string) => void;
  onOpen?: (id: string) => void;
  jumpToEdges?: boolean;
}) {
  const id = ids[index];
  function rowKey(event: KeyboardEvent<HTMLTableRowElement>) {
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' && onOpen) {
      event.preventDefault();
      onOpen(id);
      return;
    }
    let next = index;
    if (event.key === 'ArrowDown') next++;
    else if (event.key === 'ArrowUp') next--;
    else if (jumpToEdges && event.key === 'Home') next = 0;
    else if (jumpToEdges && event.key === 'End') next = ids.length - 1;
    else if (onOpen || (event.key !== 'Enter' && event.key !== ' ')) return;
    event.preventDefault();
    const nextId = ids[Math.max(0, Math.min(ids.length - 1, next))];
    onPreview(nextId);
    document.getElementById(`${idPrefix}${nextId}`)?.focus();
  }
  return (
    <Tooltip
      as="tr"
      {...row}
      id={`${idPrefix}${id}`}
      tabIndex={0}
      text={text}
      className={
        active
          ? [selectedClassName, 'position-preview-selected']
              .filter(Boolean)
              .join(' ')
          : undefined
      }
      onClick={(event) => {
        event.currentTarget.focus();
        onPreview(id);
      }}
      onDoubleClick={
        onOpen &&
        ((event) => {
          if (!(event.target as HTMLElement).closest('button,a,input'))
            onOpen(id);
        })
      }
      onKeyDown={rowKey}
    />
  );
}

export function SearchResultPlayers({
  player,
  opponent,
}: {
  player: string;
  opponent: string;
}) {
  return (
    <>
      <td className="position-player">{player}</td>
      <td className="position-opponent">{opponent}</td>
    </>
  );
}

export function SearchResultTags({ tags }: { tags: readonly string[] }) {
  return tags.map((tag) => (
    <span className="tag-chip" key={tag}>
      {tag}
    </span>
  ));
}

export function SearchResultNotes({
  tags,
  comment,
}: {
  tags?: readonly string[];
  comment?: string | null;
}) {
  return (
    <div className="position-row-notes">
      {!!tags?.length && (
        <span className="position-row-tags">
          <SearchResultTags tags={tags} />
        </span>
      )}
      <span className="position-row-comment">{comment}</span>
    </div>
  );
}

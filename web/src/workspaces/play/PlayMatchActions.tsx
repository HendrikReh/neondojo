import { useId, useRef, type ReactNode, type RefObject } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { useLibraryText } from '../library/library-messages';
import { usePlayText } from './play-messages';
import { usePlayResignText } from './play-resign-messages';

/** Pause, resign and the menu above the live match. */
export default function PlayMatchActions({
  pause,
  resign,
  interrupted,
  paused,
  busy,
  reviewing,
  resumeRef,
  menuRef,
  canForfeit,
  canStartMatch,
  onResume,
  onForfeit,
  onNewMatch,
  onOpenPausedMatches,
}: {
  pause: ReactNode;
  resign: ReactNode;
  interrupted: boolean;
  paused: boolean;
  busy: boolean;
  reviewing: boolean;
  resumeRef: RefObject<HTMLButtonElement | null>;
  /** The button that opens the menu. */
  menuRef: RefObject<HTMLButtonElement | null>;
  canForfeit: boolean;
  canStartMatch: boolean;
  onResume: () => void;
  onForfeit: () => void;
  onNewMatch: () => void;
  onOpenPausedMatches: () => void;
}) {
  const t = usePlayText();
  const resignT = usePlayResignText();
  const libraryT = useLibraryText();
  const menuId = useId();
  const menu = useRef<HTMLDivElement>(null);
  const closeMenu = () => menu.current?.hidePopover();
  return (
    <div className="play-actions play-match-actions">
      {pause && (
        <span
          className="play-pause-slot"
          aria-hidden={interrupted}
          inert={interrupted}
        >
          {resign}
          {pause}
        </span>
      )}
      {paused && (
        <ActionButton
          ref={resumeRef}
          className="primary"
          onClick={onResume}
          disabled={busy}
          icon="resume"
          label={t('resumeMatch')}
        />
      )}
      <ActionButton
        ref={menuRef}
        type="button"
        className="play-menu-button"
        icon="more"
        label={t('moreActions')}
        compact
        popoverTarget={menuId}
        disabled={busy}
      />
      <div
        id={menuId}
        ref={menu}
        popover="auto"
        role="group"
        aria-label={t('moreActions')}
        className="play-menu"
        onKeyDown={(event) => {
          if (event.key !== 'Escape') return;
          event.preventDefault();
          event.stopPropagation();
          closeMenu();
          menuRef.current?.focus();
        }}
      >
        {canForfeit && !interrupted && (
          <ActionButton
            type="button"
            icon="resign"
            label={resignT('forfeitMatch')}
            disabled={busy || reviewing}
            aria-haspopup="dialog"
            onClick={() => {
              closeMenu();
              onForfeit();
            }}
          />
        )}
        {canStartMatch && (
          <ActionButton
            type="button"
            onClick={() => {
              closeMenu();
              onNewMatch();
            }}
            disabled={busy}
            icon="new-game"
            label={t('newMatch')}
          />
        )}
        <ActionButton
          type="button"
          onClick={() => {
            closeMenu();
            onOpenPausedMatches();
          }}
          disabled={busy}
          icon="open"
          label={libraryT('pausedMatches')}
        />
      </div>
    </div>
  );
}

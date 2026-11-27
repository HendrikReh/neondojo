import ActionButton from './ActionButton';
import { useEffect, useId, useRef, type ReactNode } from 'react';
import { useLatest } from '../hooks/useLatest';
import { useLibraryText } from '../../workspaces/library/library-messages';
import {
  lockBodyScroll,
  resolveInitialFocus,
  restoreDialogFocus,
  trapTabKey,
} from './ModalDialog';

export default function LibraryDialog({
  title,
  onClose,
  busy = false,
  children,
  native = false,
  initialFocus = 'first-control',
  lockScroll = true,
}: {
  title: string;
  onClose: () => void;
  busy?: boolean;
  children: ReactNode;
  native?: boolean;
  initialFocus?: 'first-control' | 'first-field' | 'dialog';
  lockScroll?: boolean;
}) {
  const id = useId(),
    box = useRef<HTMLDivElement>(null),
    nativeLayer = useRef<HTMLDialogElement>(null),
    close = useLatest(onClose),
    pending = useLatest(busy);
  const l = useLibraryText();
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const layer = native ? nativeLayer.current : null;
    layer?.showModal();
    resolveInitialFocus(box.current, initialFocus)?.focus();
    function key(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        if (!pending.current) close.current();
      }
      if (e.key === 'Tab') {
        trapTabKey(e, box.current, document.activeElement, box.current);
      }
    }
    document.addEventListener('keydown', key, true);
    const unlock = lockScroll ? lockBodyScroll() : undefined;
    return () => {
      document.removeEventListener('keydown', key, true);
      unlock?.();
      layer?.close();
      restoreDialogFocus(previous);
    };
  }, [native, initialFocus, lockScroll, close, pending]);
  const content = (
    <div
      className="library-modal-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onClose();
      }}
    >
      <div
        ref={box}
        className="library-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={id}
        tabIndex={-1}
      >
        <div className="section-heading">
          <h2 id={id}>{title}</h2>
          <ActionButton
            icon="close"
            label={l('close')}
            compact
            type="button"
            onClick={onClose}
            disabled={busy}
            aria-label={l('close')}
          />
        </div>
        {children}
      </div>
    </div>
  );
  return native ? (
    <dialog
      ref={nativeLayer}
      role="presentation"
      className="library-native-layer"
      onCancel={(e) => {
        e.preventDefault();
        if (!pending.current) close.current();
      }}
    >
      {content}
    </dialog>
  ) : (
    content
  );
}

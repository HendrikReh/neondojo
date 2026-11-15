import ActionButton from './ActionButton';
import { useEffect, useId, useRef, type ReactNode } from 'react';
import { useLibraryText } from './library-messages';

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
    close = useRef(onClose),
    pending = useRef(busy);
  const l = useLibraryText();
  close.current = onClose;
  pending.current = busy;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const layer = native ? nativeLayer.current : null;
    layer?.showModal();
    const controls = () =>
      Array.from(
        box.current?.querySelectorAll<HTMLElement>(
          'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),a[href],[tabindex="0"]',
        ) ?? [],
      ).filter((el) => el.getClientRects().length > 0);
    const initial =
      initialFocus === 'first-field'
        ? controls().find((control) => control.matches('input,select,textarea'))
        : initialFocus === 'first-control'
          ? controls()[0]
          : undefined;
    (initial ?? box.current)?.focus();
    function key(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        if (!pending.current) close.current();
      }
      if (e.key === 'Tab') {
        const list = controls(),
          first = list[0],
          last = list.at(-1);
        if (!first) {
          e.preventDefault();
          box.current?.focus();
        } else if (
          e.shiftKey &&
          (document.activeElement === first ||
            !box.current?.contains(document.activeElement))
        ) {
          e.preventDefault();
          last?.focus();
        } else if (
          !e.shiftKey &&
          (document.activeElement === last ||
            !box.current?.contains(document.activeElement))
        ) {
          e.preventDefault();
          first.focus();
        }
      }
    }
    document.addEventListener('keydown', key, true);
    const overflow = document.body.style.overflow;
    if (lockScroll) document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', key, true);
      if (lockScroll) document.body.style.overflow = overflow;
      layer?.close();
      if (previous?.isConnected) previous.focus();
    };
  }, [native, initialFocus, lockScroll]);
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

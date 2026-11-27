import ActionIcon from '../ui/action-icons';
import ActionButton from '../ui/ActionButton';
import {
  isBackdropPointerEvent,
  restoreDialogFocus,
  trapTabKey,
} from '../ui/ModalDialog';
import {
  useEffect,
  useCallback,
  useId,
  useRef,
  type ReactNode,
  type RefObject,
  type PointerEvent,
} from 'react';
import { usePlayText } from '../../workspaces/play/play-messages';
type Center = { x: number; y: number };

/**
 * Board-anchored or viewport-centered modal; informational views may be closed.
 * With `modal={false}` the dialog leaves the rest of the page usable: the
 * caller makes the content it covers inert, and `focusOnOpen={false}` keeps
 * the focus where it is when the dialog was not opened by the user.
 */
export default function PlayBoardDialog({
  anchor,
  heading,
  busy,
  children,
  className = '',
  onClose,
  returnFocus,
  modal = true,
  focusOnOpen = true,
}: {
  anchor?: RefObject<HTMLDivElement | null>;
  returnFocus?: RefObject<HTMLElement | null>;
  heading: string;
  busy: boolean;
  children: ReactNode;
  className?: string;
  onClose?: () => void;
  modal?: boolean;
  focusOnOpen?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const titleId = useId();
  const t = usePlayText();
  const manualCenter = useRef<Center | null>(null);
  const backdropPress = useRef(false);
  const drag = useRef<{
    pointerId: number;
    offsetX: number;
    offsetY: number;
  } | null>(null);
  const place = useCallback((center: Center) => {
    const element = dialog.current;
    if (!element) return null;
    const halfWidth = element.offsetWidth / 2,
      halfHeight = element.offsetHeight / 2;
    const x = Math.max(
      halfWidth + 16,
      Math.min(center.x, window.innerWidth - halfWidth - 16),
    );
    const y = Math.max(
      halfHeight + 16,
      Math.min(center.y, window.innerHeight - halfHeight - 16),
    );
    element.style.left = `${x}px`;
    element.style.top = `${y}px`;
    return { x, y };
  }, []);
  function finishDrag(event: PointerEvent<HTMLHeadingElement>) {
    if (drag.current?.pointerId !== event.pointerId) return;
    drag.current = null;
    if (dialog.current) delete dialog.current.dataset.dragging;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  }
  useEffect(() => {
    const element = dialog.current;
    const board = anchor?.current?.querySelector('.board') ?? anchor?.current;
    if (!element) return;
    const previous =
      modal || focusOnOpen
        ? document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null
        : null;
    if (modal) element.showModal();
    // show() would move the focus; the attribute opens without focusing.
    else element.open = true;
    function position() {
      const bounds = board?.getBoundingClientRect();
      const center = place(
        manualCenter.current ?? {
          x: bounds ? bounds.x + bounds.width / 2 : window.innerWidth / 2,
          y: bounds ? bounds.y + bounds.height / 2 : window.innerHeight / 2,
        },
      );
      if (manualCenter.current) manualCenter.current = center;
    }
    position();
    if (modal || focusOnOpen) title.current?.focus({ preventScroll: true });
    const observer = new ResizeObserver(position);
    if (board) observer.observe(board);
    observer.observe(element);
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
      element.close();
      restoreDialogFocus(returnFocus?.current, () => previous, {
        preventScroll: true,
      });
    };
    // The dialog opens once; its modality and initial focus are fixed then.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anchor, place, returnFocus]);
  return (
    <dialog
      ref={dialog}
      className={`play-interruption-dialog ${className}`}
      role={onClose ? 'dialog' : 'alertdialog'}
      aria-modal={modal}
      aria-labelledby={titleId}
      aria-busy={busy}
      onPointerDown={(event) => {
        backdropPress.current =
          modal &&
          Boolean(onClose) &&
          event.button === 0 &&
          isBackdropPointerEvent({
            target: event.target,
            currentTarget: event.currentTarget,
            clientX: event.clientX,
            clientY: event.clientY,
            detail: 1,
          });
      }}
      onClick={(event) => {
        const startedOnBackdrop = backdropPress.current;
        backdropPress.current = false;
        if (
          modal &&
          startedOnBackdrop &&
          onClose &&
          !busy &&
          isBackdropPointerEvent(event)
        ) {
          onClose();
        }
      }}
      onCancel={(event) => {
        event.preventDefault();
        onClose?.();
      }}
      onKeyDown={(event) => {
        // Only a modal dialog receives the browser's cancel event.
        if (!modal && event.key === 'Escape' && onClose) {
          event.preventDefault();
          event.stopPropagation();
          onClose();
          return;
        }
        if (event.key !== 'Tab' || !modal) return;
        trapTabKey(
          event,
          event.currentTarget,
          document.activeElement,
          title.current,
        );
      }}
    >
      <div className="play-dialog-heading">
        <h2
          id={titleId}
          ref={title}
          tabIndex={-1}
          className="play-dialog-drag-handle"
          title={t('dragDialog')}
          onPointerDown={(event) => {
            if (!event.isPrimary || event.button !== 0 || !dialog.current)
              return;
            const bounds = dialog.current.getBoundingClientRect();
            const center = {
              x: bounds.x + bounds.width / 2,
              y: bounds.y + bounds.height / 2,
            };
            manualCenter.current = center;
            drag.current = {
              pointerId: event.pointerId,
              offsetX: event.clientX - center.x,
              offsetY: event.clientY - center.y,
            };
            event.currentTarget.setPointerCapture(event.pointerId);
            dialog.current.dataset.dragging = 'true';
            event.preventDefault();
          }}
          onPointerMove={(event) => {
            const active = drag.current;
            if (!active || active.pointerId !== event.pointerId) return;
            manualCenter.current = place({
              x: event.clientX - active.offsetX,
              y: event.clientY - active.offsetY,
            });
          }}
          onPointerUp={finishDrag}
          onPointerCancel={finishDrag}
          onLostPointerCapture={finishDrag}
        >
          <ActionIcon name="grip" className="play-dialog-grip" size={16} />
          {heading}
        </h2>
        {onClose && (
          <ActionButton
            type="button"
            className="play-dialog-close"
            onClick={onClose}
            icon="close"
            label={t('closeDialog')}
            compact
          ></ActionButton>
        )}
      </div>
      {children}
    </dialog>
  );
}

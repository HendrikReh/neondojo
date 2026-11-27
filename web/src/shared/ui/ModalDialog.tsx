import {
  useEffect,
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type RefObject,
} from 'react';
import { useLatest } from '../hooks/useLatest';

export const FOCUSABLE_SELECTOR =
  'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),a[href],summary,[tabindex="0"]';

export function focusableElements(container: ParentNode | null): HTMLElement[] {
  if (!container) return [];
  return Array.from(
    container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter(
    (element) =>
      !element.matches(':disabled') && element.getClientRects().length > 0,
  );
}

export function trapTabKey(
  event: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'preventDefault'>,
  container: HTMLElement | null,
  activeElement: Element | null = document.activeElement,
  fallbackFocus: HTMLElement | null = container,
): boolean {
  if (event.key !== 'Tab' || !container) return false;
  const list = focusableElements(container);
  const first = list[0];
  const last = list.at(-1);
  if (!first || !last) {
    event.preventDefault();
    fallbackFocus?.focus();
    return true;
  }
  const inside = activeElement !== null && container.contains(activeElement);
  if (event.shiftKey && (activeElement === first || !inside)) {
    event.preventDefault();
    last.focus();
    return true;
  }
  if (!event.shiftKey && (activeElement === last || !inside)) {
    event.preventDefault();
    first.focus();
    return true;
  }
  return false;
}

let scrollLockCount = 0;
let savedBodyOverflow = '';

export function lockBodyScroll(): () => void {
  if (typeof document === 'undefined' || !document.body) return () => {};
  if (scrollLockCount === 0) {
    savedBodyOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
  }
  scrollLockCount += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    scrollLockCount = Math.max(0, scrollLockCount - 1);
    if (scrollLockCount === 0) {
      document.body.style.overflow = savedBodyOverflow;
    }
  };
}

export type DialogInitialFocus =
  | 'auto'
  | 'first-control'
  | 'first-field'
  | 'dialog'
  | RefObject<HTMLElement | null>;

export function resolveInitialFocus(
  container: HTMLElement | null,
  initialFocus: DialogInitialFocus,
): HTMLElement | null | undefined {
  if (initialFocus === 'auto') return undefined;
  if (typeof initialFocus === 'object') return initialFocus.current;
  if (initialFocus === 'dialog') return container;
  const controls = focusableElements(container);
  if (initialFocus === 'first-field')
    return (
      controls.find((control) => control.matches('input,select,textarea')) ??
      container
    );
  return controls[0] ?? container;
}

export default function ModalDialog({
  open = true,
  onCancel,
  busy = false,
  lockScroll = true,
  restoreFocus = true,
  restoreFocusFallback,
  initialFocus = 'auto',
  dialogRef,
  className,
  role,
  ariaLabelledBy,
  ariaDescribedBy,
  ariaLabel,
  stopKeyPropagation = true,
  onKeyDown,
  onOpen,
  children,
}: {
  open?: boolean;
  onCancel: () => void;
  busy?: boolean;
  lockScroll?: boolean;
  restoreFocus?: boolean;
  restoreFocusFallback?: () => HTMLElement | null;
  initialFocus?: DialogInitialFocus;
  dialogRef?: RefObject<HTMLDialogElement | null>;
  className?: string;
  role?: string;
  ariaLabelledBy?: string;
  ariaDescribedBy?: string;
  ariaLabel?: string;
  stopKeyPropagation?: boolean;
  onKeyDown?: (event: ReactKeyboardEvent<HTMLDialogElement>) => void;
  onOpen?: () => void;
  children: ReactNode;
}) {
  const internalRef = useRef<HTMLDialogElement>(null);
  const ref = dialogRef ?? internalRef;
  const cancelLatest = useLatest(onCancel);
  const busyLatest = useLatest(busy);
  const fallbackLatest = useLatest(restoreFocusFallback);
  const onOpenLatest = useLatest(onOpen);
  const initialFocusLatest = useLatest(initialFocus);

  useEffect(() => {
    const element = ref.current;
    if (!open || !element) return;
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    onOpenLatest.current?.();
    if (!element.open) element.showModal();
    const target = resolveInitialFocus(element, initialFocusLatest.current);
    target?.focus();
    const unlock = lockScroll ? lockBodyScroll() : undefined;
    return () => {
      unlock?.();
      if (element.open) element.close();
      if (!restoreFocus) return;
      if (previous?.isConnected && !previous.matches(':disabled')) {
        previous.focus();
      } else {
        fallbackLatest.current?.()?.focus();
      }
    };
  }, [open, lockScroll, restoreFocus, ref, onOpenLatest, initialFocusLatest, fallbackLatest]);

  return (
    <dialog
      ref={ref}
      role={role}
      className={className}
      aria-labelledby={ariaLabelledBy}
      aria-describedby={ariaDescribedBy}
      aria-label={ariaLabel}
      onCancel={(event) => {
        event.preventDefault();
        if (!busyLatest.current) cancelLatest.current();
      }}
      onKeyDown={(event) => {
        if (stopKeyPropagation) event.stopPropagation();
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
        trapTabKey(event, event.currentTarget, document.activeElement, null);
      }}
    >
      {children}
    </dialog>
  );
}

import { useEffect, useId, useRef, useState } from 'react';
import './action-controls.css';

const eventName = 'bgtutor-action-tooltip';
export type TooltipText = string | ((anchor: Element) => string | undefined);

/** A manual popover stays above native dialogs without changing the trigger's DOM layout. */
export function useActionTooltip(text?: TooltipText, disabled = false) {
  const id = useId();
  const anchor = useRef<Element | null>(null),
    popup = useRef<HTMLSpanElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const [open, setOpen] = useState(false);
  const [measuredText, setMeasuredText] = useState<string>();
  const resolvedText = typeof text === 'function' ? measuredText : text;
  const enabled = Boolean(text) && !disabled;
  const visible = enabled && open && Boolean(resolvedText);
  function show() {
    if (typeof text === 'function' && anchor.current) {
      const content = text(anchor.current);
      setMeasuredText(content);
      setOpen(Boolean(content));
    } else setOpen(true);
  }
  function cancelTimer() {
    if (timer.current !== undefined) {
      clearTimeout(timer.current);
      timer.current = undefined;
    }
  }
  function cancelClose() {
    if (closeTimer.current !== undefined) {
      clearTimeout(closeTimer.current);
      closeTimer.current = undefined;
    }
  }
  function scheduleClose() {
    cancelTimer();
    cancelClose();
    if (document.activeElement !== anchor.current)
      closeTimer.current = setTimeout(() => setOpen(false), 180);
  }
  function dismiss() {
    cancelClose();
    cancelTimer();
    setOpen(false);
  }
  useEffect(
    () => () => {
      cancelTimer();
      cancelClose();
    },
    [],
  );
  useEffect(() => {
    if (!enabled) dismiss();
  }, [enabled]);
  useEffect(() => {
    if (!visible || !anchor.current || !popup.current) return;
    const element = popup.current,
      trigger = anchor.current;
    const closeOther = (event: Event) => {
      if ((event as CustomEvent<string>).detail !== id) dismiss();
    };
    document.addEventListener(eventName, closeOther);
    document.dispatchEvent(new CustomEvent(eventName, { detail: id }));
    element.showPopover();
    const position = () => {
      if (!trigger.isConnected || !trigger.checkVisibility()) {
        dismiss();
        return;
      }
      const bounds = trigger.getBoundingClientRect(),
        width = element.offsetWidth,
        height = element.offsetHeight;
      element.style.left = `${Math.max(8, Math.min(bounds.x + bounds.width / 2 - width / 2, window.innerWidth - width - 8))}px`;
      element.style.top = `${Math.max(8, bounds.bottom + 8 + height <= window.innerHeight - 8 ? bounds.bottom + 8 : bounds.top - height - 8)}px`;
    };
    position();
    const observer = new ResizeObserver(position);
    observer.observe(element);
    observer.observe(trigger);
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        dismiss();
      }
    };
    window.addEventListener('keydown', escape, true);
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    return () => {
      document.removeEventListener(eventName, closeOther);
      observer.disconnect();
      window.removeEventListener('keydown', escape, true);
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
      if (element.isConnected && element.matches(':popover-open'))
        element.hidePopover();
    };
  }, [visible, id, resolvedText]);
  return {
    anchor,
    describedBy: visible ? id : undefined,
    onFocus: () => {
      cancelClose();
      cancelTimer();
      if (enabled) show();
    },
    onBlur: dismiss,
    onPointerEnter: () => {
      cancelClose();
      cancelTimer();
      if (enabled) timer.current = setTimeout(show, 300);
    },
    onPointerLeave: () => {
      if (enabled) scheduleClose();
    },
    dismiss,
    content: visible ? (
      <span
        ref={popup}
        id={id}
        popover="manual"
        role="tooltip"
        tabIndex={-1}
        className="action-tooltip"
        onPointerEnter={cancelClose}
        onPointerLeave={scheduleClose}
        onPointerDown={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
      >
        {resolvedText}
      </span>
    ) : null,
  };
}

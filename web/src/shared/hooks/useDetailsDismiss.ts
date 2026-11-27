import {
  useEffect,
  useRef,
  type FocusEvent,
  type KeyboardEvent,
} from 'react';

/**
 * Makes a `<details>` behave like a popover: a pointer press outside or
 * moving keyboard focus outside closes it, and Escape closes it and returns
 * focus to its summary.
 */
export function useDetailsDismiss() {
  const ref = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (
        ref.current?.open &&
        event.target instanceof Node &&
        !ref.current.contains(event.target)
      )
        ref.current.open = false;
    };
    document.addEventListener('pointerdown', close);
    return () => document.removeEventListener('pointerdown', close);
  }, []);
  function close() {
    if (ref.current) ref.current.open = false;
  }
  function onKeyDown(event: KeyboardEvent<HTMLDetailsElement>) {
    if (event.key === 'Escape' && ref.current?.open) {
      event.preventDefault();
      ref.current.open = false;
      ref.current.querySelector('summary')?.focus();
    }
  }
  function onBlur(event: FocusEvent<HTMLDetailsElement>) {
    if (
      ref.current?.open &&
      event.relatedTarget instanceof Node &&
      !event.currentTarget.contains(event.relatedTarget)
    ) {
      ref.current.open = false;
    }
  }
  return { ref, onKeyDown, onBlur, close };
}

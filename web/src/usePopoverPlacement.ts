import { useLayoutEffect, type RefObject } from 'react';

/** Keeps an open popover menu next to its trigger and inside the window. */
export function usePopoverPlacement(
  open: boolean,
  trigger: RefObject<HTMLElement | null>,
  menu: RefObject<HTMLElement | null>,
) {
  useLayoutEffect(() => {
    const popover = menu.current;
    const anchor = trigger.current;
    if (!open || !popover || !anchor) return;
    const gap = 4;
    const edge = 8;
    const place = () => {
      const bounds = anchor.getBoundingClientRect();
      const below = bounds.bottom + gap;
      const top =
        below + popover.offsetHeight <= window.innerHeight - edge
          ? below
          : Math.max(edge, bounds.top - popover.offsetHeight - gap);
      const left = Math.max(
        edge,
        Math.min(bounds.left, window.innerWidth - popover.offsetWidth - edge),
      );
      popover.style.top = `${top}px`;
      popover.style.left = `${left}px`;
      popover.style.visibility = 'visible';
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(popover);
    if (anchor.parentElement) observer.observe(anchor.parentElement);
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
      popover.style.removeProperty('visibility');
    };
  }, [open, trigger, menu]);
}

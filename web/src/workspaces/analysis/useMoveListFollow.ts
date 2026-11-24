import { useEffect, useRef } from 'react';

/**
 * Keeps the current move of a list in view. Showing another place, such as
 * another move or filter, always brings it into view. Content that arrives
 * later does so only while the list is where this left it: a list the user
 * scrolled must not move under the pointer.
 */
export function useMoveListFollow(place: string, content: readonly unknown[]) {
  const list = useRef<HTMLDivElement>(null);
  const current = useRef<HTMLButtonElement>(null);
  const followed = useRef<{ place: string; top: number }>(undefined);
  useEffect(() => {
    const box = list.current,
      row = current.current;
    if (!box || !row || !box.clientHeight) return;
    const last = followed.current;
    // Browsers round scroll positions, so compare them with a tolerance.
    if (last?.place === place && Math.abs(box.scrollTop - last.top) > 1) return;
    const outer = box.getBoundingClientRect();
    const result = row.nextElementSibling;
    const top = row.getBoundingClientRect().top;
    const bottom = result?.matches('.game-result-summary')
      ? result.getBoundingClientRect().bottom
      : row.getBoundingClientRect().bottom;
    if (top < outer.top) box.scrollTop -= outer.top - top;
    else if (bottom > outer.bottom) box.scrollTop += bottom - outer.bottom;
    followed.current = { place, top: box.scrollTop };
    // The content is compared item by item, like the dependencies it stands for.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [place, ...content]);
  return { list, current };
}

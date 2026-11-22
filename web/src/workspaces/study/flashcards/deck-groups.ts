/** Anki-style deck paths use "::" between levels. */
const SEPARATOR = '::';

export type DeckGroup<T> = {
  /** Parent path for display, or '' for decks without a parent. */
  path: string;
  decks: { deck: T; label: string }[];
};

/**
 * Group decks under their parent path and keep only decks whose full name
 * contains the filter text. The original order is preserved within a group.
 */
export function groupDecks<T extends { id: string; name: string }>(
  decks: readonly T[],
  filter = '',
  keep?: string,
): DeckGroup<T>[] {
  const needle = filter.trim().toLocaleLowerCase();
  const groups = new Map<string, DeckGroup<T>>();
  for (const deck of decks) {
    if (
      needle &&
      deck.id !== keep &&
      !deck.name.toLocaleLowerCase().includes(needle)
    )
      continue;
    const parts = deck.name.split(SEPARATOR).map((part) => part.trim());
    const label = parts.pop() || deck.name;
    const path = parts.filter(Boolean).join(' › ');
    const group = groups.get(path) ?? { path, decks: [] };
    group.decks.push({ deck, label });
    groups.set(path, group);
  }
  // Top-level decks first, then paths in their original order.
  return [...groups.values()].sort(
    (a, b) => Number(a.path !== '') - Number(b.path !== ''),
  );
}

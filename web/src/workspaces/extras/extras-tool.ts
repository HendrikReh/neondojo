// The chosen Extras tool, remembered across sessions. App reads it without
// loading the Extras workspace.
export const extrasTools = [
  'benchmark',
  'matches',
  'backup',
  'versions',
] as const;
export type ExtrasTool = (typeof extrasTools)[number];
const storageKey = 'neondojo.extrasTool';

export function readExtrasTool(): ExtrasTool {
  try {
    const stored = localStorage.getItem(storageKey);
    return extrasTools.find((tool) => tool === stored) ?? 'benchmark';
  } catch {
    return 'benchmark';
  }
}

export function saveExtrasTool(tool: ExtrasTool) {
  try {
    localStorage.setItem(storageKey, tool);
  } catch {
    // Extras navigation also works without persistent storage.
  }
}

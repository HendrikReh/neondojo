export const DEFAULT_ANALYSIS_PLIES = 2;
export const ANALYSIS_PLIES = [0, 1, 2, 3] as const;
export function isAnalysisDepth(value: unknown) {
  return ANALYSIS_PLIES.some((depth) => depth === value);
}

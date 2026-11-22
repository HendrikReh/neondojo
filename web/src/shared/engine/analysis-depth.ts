import configurationOptions from '../../../../shared/configuration-options.json' with { type: 'json' };

/**
 * Every depth in NeonDojo uses the XG / NeonGammon convention for both
 * engines: 1-ply is the raw neural-net evaluation. The backend converts to
 * GNUbg's own level (one lower) only when it talks to GNUbg.
 */
export type DepthEngine = keyof typeof configurationOptions.engine_profiles;

/** GNUbg's former 2-ply default, unchanged in strength. */
export const DEFAULT_ANALYSIS_PLIES = 3;

export const TOP_TWO_PLAY_STRENGTH = 'top-two';

/** All analysis depths any engine supports, ascending. */
export const ANALYSIS_PLIES: readonly number[] = [
  ...new Set(
    Object.values(configurationOptions.engine_profiles).flatMap(
      (profile) => profile.analysis,
    ),
  ),
].sort((a, b) => a - b);

/** All playing strengths any engine supports, ascending. */
export const PLAY_PLIES: readonly number[] = [
  ...new Set(
    Object.values(configurationOptions.engine_profiles).flatMap(
      (profile) => profile.play,
    ),
  ),
].sort((a, b) => a - b);

/**
 * Depth chosen when switching to an engine without a remembered choice:
 * GNUbg's former 2-ply and NeonGammon's 2-ply, each in its original numbering.
 */
export function preferredDepth(engine: string) {
  return engine === 'gnubg' ? DEFAULT_ANALYSIS_PLIES : 2;
}

export function isAnalysisDepth(value: unknown) {
  return ANALYSIS_PLIES.some((depth) => depth === value);
}

/** The same depth as GNUbg itself would name it, for explanatory labels. */
export function gnubgNativePlies(plies: number) {
  return plies - 1;
}

/**
 * Option text for a depth. GNUbg depths also name GNUbg's own level so that
 * users who know GNUbg's numbering see the same search, e.g. "3-ply (GNUbg 2-ply)".
 */
export function depthLabel(plies: number, engine?: string) {
  const depth = `${plies}-ply`;
  return engine === 'gnubg'
    ? `${depth} (GNUbg ${gnubgNativePlies(plies)}-ply)`
    : depth;
}

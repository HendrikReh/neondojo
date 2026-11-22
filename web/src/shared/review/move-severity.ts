import type { components } from '../../generated-api';

export const errorGrades = [
  'doubtful',
  'bad',
  'blunder',
  'very_bad',
] as const satisfies readonly components['schemas']['MoveSeverity'][];

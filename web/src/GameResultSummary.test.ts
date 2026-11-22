import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { I18nProvider } from './shared/i18n/i18n';
import GameResultSummary, { type GameResult } from './GameResultSummary';
const gammon: GameResult = {
  winner: 1,
  points: 4,
  cube: 2,
  multiplier: 2,
  reason: 'gammon',
  scores_after: [6, 7],
};
function render(result: GameResult = gammon, locale = 'en') {
  vi.stubGlobal('localStorage', { getItem: () => locale });
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(GameResultSummary, {
        result,
        white: 'Hendrik',
        black: 'Intermediate',
      }),
    }),
  );
}
afterEach(() => vi.unstubAllGlobals());
it('explains the actual match 11 gammon and resulting match score', () => {
  const html = render();
  expect(html).toContain('Intermediate wins 4 points');
  expect(html).toContain('Gammon (2) × Cube 2 = 4 points');
  expect(html).toContain('Hendrik 6 : 7 Intermediate');
});
it('localizes the explanation in German', () => {
  const html = render(gammon, 'de');
  expect(html).toContain('Intermediate gewinnt 4 Punkte');
  expect(html).toContain('Gammon (2) × Cube 2 = 4 Punkte');
  expect(html).toContain('Spielstand');
});
it('shows single and backgammon multiplication', () => {
  expect(
    render({ ...gammon, reason: 'single', multiplier: 1, points: 2 }),
  ).toContain('Single game (1) × Cube 2 = 2 points');
  expect(
    render({ ...gammon, reason: 'backgammon', multiplier: 3, points: 6 }),
  ).toContain('Backgammon (3) × Cube 2 = 6 points');
});
it('explains cube drops at the awarded cube value', () => {
  const html = render({ ...gammon, reason: 'drop', multiplier: 1, points: 2 });
  expect(html).toContain('Double declined · Cube 2 = 2 points');
  expect(html).not.toContain('Gammon');
});
it('does not invent a gammon formula for recorded or unclassified resignation results', () => {
  for (const reason of ['recorded', 'resignation'] as const) {
    const html = render({ ...gammon, reason, multiplier: null });
    expect(html).toContain('4 points');
    expect(html).toContain('7 − 3 = 4 points');
    expect(html).not.toContain('×');
    expect(html).not.toContain('Gammon');
  }
});
it('uses singular point text for a one-point win', () => {
  const html = render({
    ...gammon,
    points: 1,
    cube: 1,
    multiplier: 1,
    reason: 'single',
  });
  expect(html).toContain('Intermediate wins 1 point<');
  expect(html).toContain('Single game (1) × Cube 1 = 1 point<');
});

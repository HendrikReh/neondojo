import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { GamePlanPanelView } from './GamePlanPanel';
import { gamePlanMessages } from './game-plan-messages';
import { I18nProvider, type Locale } from '../../../shared/i18n/i18n';
import type { GamePlans } from './game-plan-api';

const plans: GamePlans = {
  classifier: 'opensage_rules_v1_opening1',
  source_revision: 'revision',
  game_over: false,
  white: 'anchoring',
  black: 'attacking',
};
function render(
  overrides: Partial<Parameters<typeof GamePlanPanelView>[0]> = {},
  locale: Locale = 'en',
) {
  vi.stubGlobal('localStorage', { getItem: () => locale });
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(GamePlanPanelView, {
        white: 'Alice',
        black: 'Bob',
        plans,
        loading: false,
        onRetry: () => {},
        ...overrides,
      }),
    }),
  );
}
afterEach(() => vi.unstubAllGlobals());

it('shows both actual player names in fixed White/Black order with board-only attribution', () => {
  const html = render();
  expect(html).toContain('Game plan');
  expect(html).toMatch(
    /data-game-plan="white"[\s\S]*White[\s\S]*Alice[\s\S]*Anchoring/,
  );
  expect(html).toMatch(
    /data-game-plan="black"[\s\S]*Black[\s\S]*Bob[\s\S]*Attacking/,
  );
  expect(html.indexOf('data-game-plan="white"')).toBeLessThan(
    html.indexOf('data-game-plan="black"'),
  );
  expect(html).toContain('NeonGammon');
  expect(html).toContain('board structure');
  expect(html).toContain('GNUbg');
  expect(html).toContain('move recommendations');
  expect(html).not.toContain('%');
  expect(html).not.toContain('optimal');
});

it('shows a finished cue instead of ongoing plans for a terminal board', () => {
  const html = render({ plans: { ...plans, game_over: true } });
  expect(html).toContain('Game finished');
  expect(html).toContain('Alice');
  expect(html).toContain('Bob');
  expect(html).not.toContain('Anchoring');
  expect(html).not.toContain('Attacking');
});

it('keeps pending and failed requests neutral and offers a localized retry', () => {
  const pending = render({ plans: undefined, loading: true });
  expect(pending).toContain('aria-busy="true"');
  expect(pending).toContain('Loading game plan');
  expect(pending).not.toContain('Anchoring');
  const failed = render(
    { plans: undefined, error: new Error('server details'), loading: false },
    'de',
  );
  expect(failed).toContain('Spielplan nicht verfügbar');
  expect(failed).toContain('Erneut versuchen');
  expect(failed).not.toContain('server details');
});

it.each([
  ['en', 'Game plan', 'Pure race', 'Racing (contact)', 'Game finished'],
  [
    'de',
    'Spielplan',
    'Reines Rennen',
    'Rennspiel (mit Kontakt)',
    'Spiel beendet',
  ],
  [
    'es',
    'Plan de juego',
    'Carrera pura',
    'Carrera (con contacto)',
    'Partida terminada',
  ],
  [
    'fr',
    'Plan de jeu',
    'Course pure',
    'Course (avec contact)',
    'Partie terminée',
  ],
  ['pl', 'Plan gry', 'Czysty wyścig', 'Wyścig (z kontaktem)', 'Gra zakończona'],
  [
    'it',
    'Piano di gioco',
    'Corsa pura',
    'Corsa (con contatto)',
    'Partita terminata',
  ],
] as const)(
  'localizes %s and distinguishes pure races from contact racing',
  (locale, title, pure, contact, finished) => {
    const html = render(
      { plans: { ...plans, white: 'pure_race', black: 'racing' } },
      locale,
    );
    expect(html).toContain(title);
    expect(html).toContain(pure);
    expect(html).toContain(contact);
    expect(render({ plans: { ...plans, game_over: true } }, locale)).toContain(
      finished,
    );
  },
);

it('provides complete nonempty messages in all six languages', () => {
  expect(Object.keys(gamePlanMessages)).toHaveLength(6);
  for (const catalog of Object.values(gamePlanMessages)) {
    expect(Object.keys(catalog)).toEqual(Object.keys(gamePlanMessages.en));
    for (const message of Object.values(catalog))
      expect(message.trim()).not.toBe('');
  }
});

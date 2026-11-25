import archiveThemes from './archive-themes';
import { createElement } from 'react';
import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import PlayBoardDice from './PlayBoardDice';
import { type BgBlitzTheme } from './themes';
import type { DiceInteraction } from './PlayDice';
const controls: DiceInteraction = {
  canSwap: false,
  swap: () => {},
  label: 'dice',
  active: null,
  used: [false, false],
  opening: {
    values: [6, 2],
    labels: ['White', 'Black'],
    unrolled: 'Not rolled',
  },
};
const render = (
  theme?: BgBlitzTheme,
  interaction = controls,
  player = 0,
  dice: number[] = [],
) =>
  renderToStaticMarkup(
    createElement(
      'svg',
      {},
      createElement(PlayBoardDice, { dice, player, interaction, theme }),
    ),
  );
it('renders one actual white/black opening die using each archive theme artwork', () => {
  for (const theme of archiveThemes) {
    const html = render(theme);
    expect(html).toContain(`href="${theme.dice.white[5].src}"`);
    expect(html).toContain(`href="${theme.dice.black[1].src}"`);
    expect(html).toContain('data-opening-player="0"');
    expect(html).toContain('data-opening-player="1"');
    expect(html).toContain(`translate(${theme.diceRightX},`);
    expect(html).toContain(`translate(${theme.diceLeftX},`);
  }
});
it('uses the ordinary built-in dice faces and opposite board halves for the opening', () => {
  const html = render(undefined, controls, 1);
  expect(html).toContain('translate(420,219)');
  expect(html).toContain('translate(185,219)');
  expect(html).toContain('white-checker');
  expect(html).toContain('black-checker');
  expect(html).toContain('⚅');
  expect(html).toContain('⚁');
  expect(html).toContain('White · 6');
  expect(html).toContain('Black · 2');
});
it('labels unrolled dice and reuses rolling animation for both player dice', () => {
  const html = render(undefined, {
    ...controls,
    rolling: true,
    opening: { ...controls.opening!, values: [null, null] },
  });
  expect(html.match(/data-opening-die="pending"/g)).toHaveLength(2);
  expect(html.match(/data-rolling="true"/g)).toHaveLength(2);
  expect(html).toContain('White · Not rolled');
  expect(html).toContain('Black · Not rolled');
});
it('renders same-color dice with one swap control after the opening', () => {
  for (const theme of archiveThemes) {
    const html = render(
      theme,
      { ...controls, opening: undefined, canSwap: true },
      1,
      [6, 2],
    );
    expect(html).toContain(`href="${theme.dice.black[5].src}"`);
    expect(html).toContain(`href="${theme.dice.black[1].src}"`);
    expect(html).not.toContain('data-opening-player');
    expect(html).toContain('data-dice-action="swap"');
    expect(html.match(/role="button"/g)).toHaveLength(1);
  }
});

it('marks consumed dice as faded artwork without active status lines', () => {
  const html = render(
    undefined,
    {
      ...controls,
      opening: undefined,
      canConfirm: true,
      confirm: () => {},
      active: 0,
      used: [true, false],
    },
    0,
    [3, 2],
  );
  expect(html).not.toContain('play-die-active');
  expect(html).toContain('data-used-fraction="1"');
  expect(html).toContain('class="play-die-artwork"');
});
it('masks exactly half a partly consumed double die for every theme', () => {
  for (const theme of [undefined, ...archiveThemes]) {
    const html = render(
      theme,
      {
        ...controls,
        opening: undefined,
        used: [0.5, false],
      },
      1,
      [4, 4],
    );
    expect(html).toContain('data-used-fraction="0.5"');
    expect(html).toContain('maskUnits="userSpaceOnUse"');
    expect(html).toContain('fill-opacity="0.28"');
    expect(html).toContain('mask="url(#');
  }
});

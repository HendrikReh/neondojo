import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import Board from './Board';
import { I18nProvider } from '../i18n/i18n';
import { moveArrowGeometry } from './MoveArrows';
import { projectMove, type BoardView } from './move-preview';
import { archiveThemes, stackGeometry } from './themes';
import type { Position } from '../http/api';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const position = (player = 0): Position => {
  const board = Array<number>(26).fill(0);
  board[player === 0 ? 13 : 12] = player === 0 ? 1 : -1;
  return {
    board,
    player,
    dice: [6, 1],
    notation: '13/6',
    id: 'position',
    position_id: 'id',
    gnubg_match_id: '',
    match_uuid: 'match',
    game_no: 1,
    ply: 1,
    ordinal: 1,
  };
};
const render = (
  source: Position,
  theme: string,
  previewMode?: BoardView,
  preview = projectMove(source),
) =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(Board, {
        position: source,
        theme,
        white: 'White',
        black: 'Black',
        ...(previewMode
          ? { preview, previewMode, previewLabel: 'Selected candidate 13/6' }
          : {}),
      }),
    }),
  );

test('retains the existing board when no preview props are supplied', () => {
  const html = render(position(), 'forest');
  expect(html).not.toContain('move-arrows');
  expect(html).not.toContain('Selected candidate');
  expect(html).toContain('⚅');
});

test('before, arrows and after distinguish original checker placement and dice', () => {
  for (const theme of ['forest', 'walnut', 'slate']) {
    const source = position();
    const before = render(source, theme, 'before');
    const arrows = render(source, theme, 'arrows');
    const after = render(source, theme, 'after');
    for (const html of [before, arrows, after])
      expect(html).toContain('aria-label="Selected candidate 13/6. ');
    expect(before).toContain('data-preview-mode="before"');
    expect(before).toContain('data-point="13" data-count="1"');
    expect(before).not.toContain('move-arrows');
    expect(arrows).toContain('class="move-arrows"');
    expect(arrows).toContain('data-point="13" data-count="1"');
    expect(arrows).toContain('data-from="13" data-to="7" data-die="6"');
    expect(arrows).toContain('data-from="7" data-to="6" data-die="1"');
    expect(after).toContain('data-preview-mode="after"');
    expect(after).toContain('data-point="13" data-count="0"');
    expect(after).toContain('data-point="6" data-count="1"');
    expect(after).not.toContain('⚅');
    expect(after).not.toContain('move-arrows');
    expect(source.board[13]).toBe(1);
  }
});

test('an invalid projection leaves the original board and dice visible', () => {
  const html = render(position(), 'forest', 'after', {
    ok: false,
    reason: 'Invalid move',
  });
  expect(html).toContain('data-preview-mode="before"');
  expect(html).toContain('data-point="13" data-count="1"');
  expect(html).toContain('⚅');
});

// CI prepares themes without the BGBlitz archives
// (NEONDOJO_ALLOW_MISSING_THEMES=1), which leaves the archive catalog empty.
const archivesOmitted =
  archiveThemes.length === 0 &&
  process.env.NEONDOJO_ALLOW_MISSING_THEMES === '1';

test.skipIf(archivesOmitted)(
  'renders before, outlined legal arrows and after across every archive theme and mover',
  () => {
    expect(archiveThemes).toHaveLength(13);
    for (const theme of archiveThemes)
      for (const player of [0, 1]) {
        const source = position(player);
        const arrows = render(source, theme.id, 'arrows');
        expect(arrows).toContain(`data-theme="${theme.id}"`);
        expect(arrows).toContain('class="move-arrows"');
        expect(arrows).toContain(
          `data-number-orientation="${player === 0 ? 'RLD' : 'RLU'}"`,
        );
        expect(arrows).toContain('data-dice="6-1"');
        const after = render(source, theme.id, 'after');
        expect(after).toContain('data-dice=""');
        expect(after).toContain(
          `data-number-orientation="${player === 0 ? 'RLD' : 'RLU'}"`,
        );
        expect(after).toContain(
          `data-point="${player === 0 ? 6 : 19}" data-count="${player === 0 ? 1 : -1}"`,
        );
      }
  },
);

test('arrow endpoints follow actual archive checker offsets and compressed stacks', () => {
  for (const theme of archiveThemes)
    for (const player of [0, 1]) {
      const source = position(player);
      const from = player === 0 ? 13 : 12;
      source.board[from] = player === 0 ? 8 : -8;
      source.notation = '13/7 13/12';
      const preview = projectMove(source);
      expect(preview.ok).toBe(true);
      if (!preview.ok) throw new Error(preview.reason);
      const arrows = moveArrowGeometry(
        source.board,
        source.player,
        preview.steps,
        theme,
      );
      const image = theme.checkers[player === 0 ? 'white' : 'black'][0];
      const geometry = stackGeometry(
        theme,
        image,
        1,
        source.board[from],
        player === 1,
      );
      expect(arrows[0].start).toEqual({
        x: geometry.x + image.width / 2,
        y:
          geometry.y +
          (player === 1 ? -1 : 1) * 4 * geometry.step +
          image.height / 2,
      });
      for (const arrow of arrows)
        for (const point of [arrow.start, arrow.end]) {
          expect(point.x).toBeGreaterThan(0);
          expect(point.x).toBeLessThan(theme.background.width);
          expect(point.y).toBeGreaterThan(0);
          expect(point.y).toBeLessThan(theme.background.height);
        }
    }
});

test('bar and off arrow endpoints remain distinct from opposing bar checkers', () => {
  for (const player of [0, 1]) {
    const source = position(player);
    source.board.fill(0);
    source.board[player === 0 ? 1 : 24] = player === 0 ? 1 : -1;
    source.board[player === 0 ? 0 : 25] = player === 0 ? -2 : 2;
    source.notation = '1/off';
    const preview = projectMove(source);
    expect(preview.ok).toBe(true);
    if (!preview.ok) throw new Error(preview.reason);
    for (const theme of [undefined, ...archiveThemes]) {
      const [arrow] = moveArrowGeometry(
        source.board,
        player,
        preview.steps,
        theme,
      );
      const barX = theme
        ? theme.columns[7] +
          theme.checkerOffsetX +
          theme.checkers[player === 0 ? 'white' : 'black'][0].width / 2
        : 340;
      expect(arrow.end.x).not.toBe(barX);
      const entry = moveArrowGeometry(
        source.board,
        player,
        [{ from: 'bar', to: player === 0 ? 24 : 1, die: 1, hit: false }],
        theme,
      )[0];
      expect(entry.start.x).toBe(barX);
      expect(entry.end.x).not.toBe(barX);
    }
  }
});

test('preview labels retain dice and pip information for screen readers', () => {
  const html = render(position(), 'forest', 'arrows');
  expect(html).toContain('dice 6 and 1');
  expect(html).toContain('White 13 pips');
  const after = render(position(), 'forest', 'after');
  expect(after).toContain('White 6 pips');
  expect(after).not.toContain('White to move');
});

import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const css: string = readFileSync(
  new URL('./ui-appearance.css', import.meta.url),
  'utf8',
);
const boardCss: string = readFileSync(
  new URL('./style.css', import.meta.url),
  'utf8',
);
const boardPreviewCss: string = readFileSync(
  new URL('../shared/board/board-preview.css', import.meta.url),
  'utf8',
);

it('defines the approved six palettes and stable semantic status colors', () => {
  const expected = {
    graphite: {
      light: ['#eef1f5', '#ffffff', '#315bc5'],
      dark: ['#15191f', '#1e242d', '#98b6ff'],
    },
    club: {
      light: ['#edf2ee', '#fbfdf9', '#30664c'],
      dark: ['#151e1a', '#1d2a23', '#acce9a'],
    },
    paper: {
      light: ['#f2eee7', '#fffcf5', '#78528e'],
      dark: ['#211d1d', '#2b2525', '#d0aae6'],
    },
  };
  for (const [theme, modes] of Object.entries(expected)) {
    for (const [mode, values] of Object.entries(modes)) {
      const block = css.match(
        new RegExp(
          `\\[data-ui-theme='${theme}'\\]\\[data-ui-mode='${mode}'\\]\\s*\\{([^}]+)\\}`,
        ),
      )?.[1];
      expect(block).toBeDefined();
      for (const value of values) expect(block).toContain(value);
      expect(block).not.toContain('--ui-danger');
    }
  }
  for (const token of ['success', 'warning', 'danger']) {
    expect(css).toContain(`--ui-${token}-text:`);
    expect(css).toContain(`--ui-${token}-bg:`);
  }
});

it('retains the original board and checker palette independently of interface tokens', () => {
  expect(boardCss).toContain('--frame: #234b3d');
  expect(boardCss).toContain('--felt: #e3dec7');
  expect(boardCss).toContain('--black: #593d30');
  expect(boardCss).toContain('fill: var(--white)');
  expect(boardPreviewCss).toContain('fill: #28be68');
});

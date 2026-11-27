import { expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';

const css: string = readFileSync(
  new URL('./ui-appearance.css', import.meta.url),
  'utf8',
);
const boardCss: string = readFileSync(
  new URL('../shared/board/board.css', import.meta.url),
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

const baseCss: string = readFileSync(
  new URL('./base.css', import.meta.url),
  'utf8',
);

function relativeLuminance(hex: string) {
  const value = hex.replace('#', '');
  const channels = [0, 2, 4].map((offset) => {
    const channel = parseInt(value.slice(offset, offset + 2), 16) / 255;
    return channel <= 0.04045
      ? channel / 12.92
      : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0]! + 0.7152 * channels[1]! + 0.0722 * channels[2]!;
}

function contrastRatio(first: string, second: string) {
  const a = relativeLuminance(first);
  const b = relativeLuminance(second);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

function tokenHex(block: string, token: string) {
  const match = block.match(new RegExp(`${token}:\\s*(#[0-9a-fA-F]{6})\\b`));
  expect(match, `missing ${token}`).not.toBeNull();
  return match![1]!;
}

it('meets WCAG 2.1 SC 1.4.11 (>= 3:1) for interactive control borders across all six palettes', () => {
  for (const theme of ['graphite', 'club', 'paper']) {
    for (const mode of ['light', 'dark']) {
      const block = css.match(
        new RegExp(
          `\\[data-ui-theme='${theme}'\\]\\[data-ui-mode='${mode}'\\]\\s*\\{([^}]+)\\}`,
        ),
      )?.[1];
      expect(block).toBeDefined();
      const controlBorder = tokenHex(block!, '--ui-control-border');
      const surface = tokenHex(block!, '--ui-surface');
      const surfaceAlt = tokenHex(block!, '--ui-surface-alt');
      const bg = tokenHex(block!, '--ui-bg');
      expect(
        contrastRatio(controlBorder, surface),
        `${theme} ${mode} control border vs surface`,
      ).toBeGreaterThanOrEqual(3);
      expect(
        contrastRatio(controlBorder, surfaceAlt),
        `${theme} ${mode} control border vs surface-alt`,
      ).toBeGreaterThanOrEqual(3);
      expect(
        contrastRatio(controlBorder, bg),
        `${theme} ${mode} control border vs bg`,
      ).toBeGreaterThanOrEqual(3);
    }
  }
  expect(baseCss).toMatch(/button\s*\{[^}]*var\(--ui-control-border\)/);
  expect(baseCss).toMatch(/input,\s*select\s*\{[^}]*var\(--ui-control-border\)/);
  expect(baseCss).toMatch(/textarea\s*\{[^}]*var\(--ui-control-border\)/);
  expect(baseCss).toMatch(/\.search-box\s*\{[^}]*var\(--ui-control-border\)/);
});

it('retains the original board and checker palette independently of interface tokens', () => {
  expect(boardCss).toContain('--frame: #234b3d');
  expect(boardCss).toContain('--felt: #e3dec7');
  expect(boardCss).toContain('--black: #593d30');
  expect(boardCss).toContain('fill: var(--white)');
  expect(boardPreviewCss).toContain('fill: #28be68');
});

/** Custom properties that components set inline. */
const runtimeProperties = [
  '--analysis-board-width',
  '--analysis-viewport-space',
  '--card-preview-width',
  '--frame-fill',
  '--move-from-x',
  '--move-from-y',
  '--move-to-x',
  '--move-to-y',
  '--play-flip-x',
  '--play-flip-y',
];

function sources(extensions: string[]) {
  const root = new URL('../', import.meta.url);
  return readdirSync(root, { recursive: true, encoding: 'utf8' })
    .filter((path) => extensions.some((extension) => path.endsWith(extension)))
    .map((path) => ({
      path,
      text: readFileSync(new URL(path, root), 'utf8'),
    }));
}

it('defines every custom property that a stylesheet reads', () => {
  const stylesheets = sources(['.css']).map(({ path, text }) => ({
    path,
    // Keep line numbers while ignoring commented-out declarations.
    text: text.replace(/\/\*[\s\S]*?\*\//g, (comment) =>
      comment.replace(/[^\n]/g, ' '),
    ),
  }));
  const defined = new Set(
    stylesheets.flatMap(({ text }) =>
      [...text.matchAll(/(--[\w-]+)\s*:/g)].map((match) => match[1]),
    ),
  );
  const undefinedReads = stylesheets.flatMap(({ path, text }) =>
    [...text.matchAll(/var\(\s*(--[\w-]+)\s*(?:,|\))/g)]
      .filter(
        ([, name]) =>
          name !== undefined &&
          !defined.has(name) &&
          !runtimeProperties.includes(name),
      )
      .map(
        ({ 1: name, index }) =>
          `${path}:${text.slice(0, index).split('\n').length} ${name}`,
      ),
  );
  expect(undefinedReads).toEqual([]);
});

it('only exempts custom properties that components really set at runtime', () => {
  const code = sources(['.ts', '.tsx'])
    .filter(({ path }) => !path.includes('.test.'))
    .map(({ text }) => text)
    .join('\n');
  for (const name of runtimeProperties) expect(code).toContain(`'${name}'`);
});

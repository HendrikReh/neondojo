import { expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const src = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = join(src, 'styles/stylesheets.ts');

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}

/** Stylesheets a module imports, as paths relative to `src`. */
function stylesheetImports(path: string) {
  return [
    ...readFileSync(path, 'utf8').matchAll(/^import '([^']+\.css)';$/gm),
  ].map((match) => relative(src, resolve(dirname(path), match[1])));
}

const listed = stylesheetImports(manifest);
// index.html links the palette tokens ahead of the bundle.
const linked = ['styles/ui-appearance.css'];

it('lists every stylesheet exactly once', () => {
  const stylesheets = files(src)
    .filter((path) => path.endsWith('.css'))
    .map((path) => relative(src, path))
    .filter((path) => !linked.includes(path));
  expect(new Set(listed).size).toBe(listed.length);
  expect([...listed].sort()).toEqual(stylesheets.sort());
});

it('lists every stylesheet a component imports', () => {
  const imported = files(src)
    .filter((path) => /\.tsx?$/.test(path) && path !== manifest)
    .flatMap(stylesheetImports);
  expect(imported.filter((path) => !listed.includes(path))).toEqual([]);
});

it('loads the stylesheets before any component', () => {
  const main = readFileSync(join(src, 'main.tsx'), 'utf8');
  const first = main.match(/^import [^;]+;$/m)?.[0];
  expect(first).toBe("import './styles/stylesheets';");
});

import ts from 'typescript';
import { expect, it } from 'vitest';

const sources = import.meta.glob<string>('../../**/*.{ts,tsx}', {
  eager: true,
  query: '?raw',
  import: 'default',
});

const isFunction = (node: ts.Node) =>
  ts.isFunctionDeclaration(node) ||
  ts.isFunctionExpression(node) ||
  ts.isArrowFunction(node) ||
  ts.isMethodDeclaration(node);

function componentName(node: ts.Node): string {
  if (ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node))
    if (node.name) return node.name.text;
  let parent: ts.Node | undefined = node.parent;
  if (parent && ts.isCallExpression(parent)) parent = parent.parent;
  return parent && ts.isVariableDeclaration(parent)
    ? parent.name.getText()
    : '';
}

function parse(path: string, text: string) {
  return ts.createSourceFile(
    path,
    text,
    ts.ScriptTarget.Latest,
    true,
    path.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
}

/** The nodes of components and hooks that run while they render. */
function renderNodes(source: ts.SourceFile) {
  const found = new Set<ts.Node>();
  function body(node: ts.Node) {
    if (isFunction(node)) return;
    found.add(node);
    ts.forEachChild(node, body);
  }
  function visit(node: ts.Node) {
    if (isFunction(node) && /^(use[A-Z]|[A-Z])/.test(componentName(node)))
      ts.forEachChild(node, body);
    ts.forEachChild(node, visit);
  }
  visit(source);
  return [...found];
}

const place = (path: string, source: ts.SourceFile, node: ts.Node) =>
  `${path}:${source.getLineAndCharacterOfPosition(node.getStart()).line + 1}`;

/** Assignments to a ref's `current` that run while a component renders. */
function renderWrites(path: string, text: string) {
  const source = parse(path, text);
  const isRef = (node: ts.Node) => /\.current\b/.test(node.getText());
  return renderNodes(source)
    .filter(
      (node) =>
        (ts.isBinaryExpression(node) &&
          node.operatorToken.kind >= ts.SyntaxKind.FirstAssignment &&
          node.operatorToken.kind <= ts.SyntaxKind.LastAssignment &&
          isRef(node.left)) ||
        ((ts.isPostfixUnaryExpression(node) ||
          ts.isPrefixUnaryExpression(node)) &&
          (node.operator === ts.SyntaxKind.PlusPlusToken ||
            node.operator === ts.SyntaxKind.MinusMinusToken) &&
          isRef(node.operand)),
    )
    .map((node) => place(path, source, node));
}

/** The names of a file's refs, without those that hold a DOM element. */
function valueRefs(source: ts.SourceFile) {
  const names = new Set<string>();
  function visit(node: ts.Node) {
    if (
      ts.isVariableDeclaration(node) &&
      node.initializer &&
      ts.isCallExpression(node.initializer) &&
      /^(useRef|useLatest)$/.test(node.initializer.expression.getText()) &&
      !node.initializer.typeArguments?.some((type) =>
        /\b(HTML|SVG)\w*|\bElement\b/.test(type.getText()),
      )
    )
      names.add(node.name.getText());
    ts.forEachChild(node, visit);
  }
  visit(source);
  return names;
}

/** Reads of a ref's `current`, other than DOM elements, while rendering. */
function renderReads(path: string, text: string) {
  const source = parse(path, text);
  const refs = valueRefs(source);
  return renderNodes(source)
    .filter(
      (node) =>
        ts.isPropertyAccessExpression(node) &&
        node.name.text === 'current' &&
        refs.has(node.expression.getText()),
    )
    .map((node) => place(path, source, node));
}

it('finds a ref that a render writes', () => {
  const component = (line: string) =>
    renderWrites('x.tsx', `function View() { const a = useRef(0); ${line} }`);
  expect(component('a.current = 1;')).toHaveLength(1);
  expect(component('if (b) a.current++;')).toHaveLength(1);
  expect(component('useEffect(() => { a.current = 1; });')).toEqual([]);
  expect(component('function save() { a.current = 1; }')).toEqual([]);
});

it('no component or hook writes a ref while rendering', () => {
  expect(
    Object.entries(sources)
      .filter(([path]) => !/\.test\.|\.d\.ts$/.test(path))
      .flatMap(([path, text]) => renderWrites(path, text)),
  ).toEqual([]);
});

it('finds a ref that a render reads', () => {
  const component = (line: string, type = '') =>
    renderReads(
      'x.tsx',
      `function View() { const a = useRef${type}(null); ${line} }`,
    );
  expect(component('const b = a.current;')).toHaveLength(1);
  expect(component('return <p>{a.current}</p>;')).toHaveLength(1);
  expect(component('return <p onClick={() => a.current} />;')).toEqual([]);
  expect(component('useEffect(() => void a.current);')).toEqual([]);
  expect(component('const b = a.current;', '<HTMLElement>')).toEqual([]);
  expect(component('const b = config.current;')).toEqual([]);
  expect(
    renderReads(
      'x.ts',
      'function useB(v) { const a = useLatest(v); a.current; }',
    ),
  ).toHaveLength(1);
});

it('no component or hook reads a ref while rendering', () => {
  expect(
    Object.entries(sources)
      .filter(([path]) => !/\.test\.|\.d\.ts$/.test(path))
      .flatMap(([path, text]) => renderReads(path, text)),
  ).toEqual([]);
});

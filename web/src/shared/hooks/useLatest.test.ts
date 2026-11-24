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

/** Assignments to a ref's `current` that run while a component renders. */
function renderWrites(path: string, text: string) {
  const source = ts.createSourceFile(
    path,
    text,
    ts.ScriptTarget.Latest,
    true,
    path.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const found: string[] = [];
  const isRef = (node: ts.Node) => /\.current\b/.test(node.getText());
  function body(node: ts.Node) {
    if (isFunction(node)) return;
    const assigns =
      ts.isBinaryExpression(node) &&
      node.operatorToken.kind >= ts.SyntaxKind.FirstAssignment &&
      node.operatorToken.kind <= ts.SyntaxKind.LastAssignment &&
      isRef(node.left);
    const counts =
      (ts.isPostfixUnaryExpression(node) || ts.isPrefixUnaryExpression(node)) &&
      (node.operator === ts.SyntaxKind.PlusPlusToken ||
        node.operator === ts.SyntaxKind.MinusMinusToken) &&
      isRef(node.operand);
    if (assigns || counts)
      found.push(
        `${path}:${source.getLineAndCharacterOfPosition(node.getStart()).line + 1}`,
      );
    ts.forEachChild(node, body);
  }
  function visit(node: ts.Node) {
    if (isFunction(node) && /^(use[A-Z]|[A-Z])/.test(componentName(node)))
      ts.forEachChild(node, body);
    ts.forEachChild(node, visit);
  }
  visit(source);
  return found;
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

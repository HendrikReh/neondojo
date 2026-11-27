import ts from 'typescript';
import { expect, it } from 'vitest';

const sources = import.meta.glob<string>('../../**/*.{ts,tsx}', {
  eager: true,
  query: '?raw',
  import: 'default',
});

function parse(path: string, text: string) {
  return ts.createSourceFile(
    path,
    text,
    ts.ScriptTarget.Latest,
    true,
    path.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
}

function unwrapExpression(expr: ts.Expression): ts.Expression {
  let current = expr;
  while (
    ts.isParenthesizedExpression(current) ||
    ts.isAsExpression(current) ||
    ts.isSatisfiesExpression(current)
  ) {
    current = current.expression;
  }
  return current;
}

function returnsFreshLiteral(body: ts.ConciseBody): boolean {
  if (!ts.isBlock(body)) {
    const expr = unwrapExpression(body);
    return (
      ts.isObjectLiteralExpression(expr) || ts.isArrayLiteralExpression(expr)
    );
  }
  let found = false;
  function visit(node: ts.Node) {
    if (found) return;
    if (
      ts.isFunctionDeclaration(node) ||
      ts.isFunctionExpression(node) ||
      ts.isArrowFunction(node) ||
      ts.isMethodDeclaration(node)
    ) {
      return;
    }
    if (ts.isReturnStatement(node) && node.expression) {
      const expr = unwrapExpression(node.expression);
      if (
        ts.isObjectLiteralExpression(expr) ||
        ts.isArrayLiteralExpression(expr)
      ) {
        found = true;
        return;
      }
    }
    ts.forEachChild(node, visit);
  }
  ts.forEachChild(body, visit);
  return found;
}

/**
 * Finds custom hooks (`use[A-Z]...`) whose return statements directly allocate
 * a fresh object or array literal on every render (not memoized).
 */
function collectUnstableHooks(files: readonly [string, string][]): Set<string> {
  const unstable = new Set<string>();
  for (const [path, text] of files) {
    const source = parse(path, text);
    function visit(node: ts.Node) {
      if (
        ts.isFunctionDeclaration(node) &&
        node.name &&
        /^use[A-Z]/.test(node.name.text) &&
        node.body &&
        returnsFreshLiteral(node.body)
      ) {
        unstable.add(node.name.text);
      } else if (
        ts.isVariableDeclaration(node) &&
        ts.isIdentifier(node.name) &&
        /^use[A-Z]/.test(node.name.text) &&
        node.initializer &&
        (ts.isArrowFunction(node.initializer) ||
          ts.isFunctionExpression(node.initializer)) &&
        returnsFreshLiteral(node.initializer.body)
      ) {
        unstable.add(node.name.text);
      }
      ts.forEachChild(node, visit);
    }
    visit(source);
  }
  return unstable;
}

const EFFECT_HOOKS = new Set(['useEffect', 'useLayoutEffect']);

/**
 * Flags `useEffect`/`useLayoutEffect` dependency arrays that reference either
 * an inline object/array literal or a variable bound to an unstable custom hook
 * return object (the exact bug class from commit 1097416).
 */
function findUnstableEffectDeps(
  path: string,
  text: string,
  unstableHooks: ReadonlySet<string>,
): string[] {
  const source = parse(path, text);
  const violations: string[] = [];

  const place = (node: ts.Node, reason: string) => {
    const { line } = source.getLineAndCharacterOfPosition(
      node.getStart(source),
    );
    return `${path}:${line + 1} (${reason})`;
  };

  function scanScope(root: ts.Node, inherited: ReadonlyMap<string, string>) {
    const unstableVars = new Map(inherited);

    function collectBindings(node: ts.Node) {
      if (
        node !== root &&
        (ts.isFunctionDeclaration(node) ||
          ts.isFunctionExpression(node) ||
          ts.isArrowFunction(node) ||
          ts.isMethodDeclaration(node))
      ) {
        return;
      }
      if (
        ts.isVariableDeclaration(node) &&
        ts.isIdentifier(node.name) &&
        node.initializer
      ) {
        const init = unwrapExpression(node.initializer);
        if (ts.isCallExpression(init)) {
          const callee = ts.isIdentifier(init.expression)
            ? init.expression.text
            : ts.isPropertyAccessExpression(init.expression)
              ? init.expression.name.text
              : '';
          if (unstableHooks.has(callee)) {
            unstableVars.set(node.name.text, callee);
          }
        }
      }
      ts.forEachChild(node, collectBindings);
    }
    collectBindings(root);

    function inspect(node: ts.Node) {
      if (
        node !== root &&
        (ts.isFunctionDeclaration(node) ||
          ts.isFunctionExpression(node) ||
          ts.isArrowFunction(node) ||
          ts.isMethodDeclaration(node))
      ) {
        scanScope(node, unstableVars);
        return;
      }
      if (ts.isCallExpression(node)) {
        const callee = ts.isIdentifier(node.expression)
          ? node.expression.text
          : ts.isPropertyAccessExpression(node.expression)
            ? node.expression.name.text
            : '';
        if (EFFECT_HOOKS.has(callee) && node.arguments.length >= 2) {
          const depsArg = unwrapExpression(node.arguments[1]);
          if (ts.isArrayLiteralExpression(depsArg)) {
            for (const element of depsArg.elements) {
              const expr = unwrapExpression(element);
              if (
                ts.isObjectLiteralExpression(expr) ||
                ts.isArrayLiteralExpression(expr)
              ) {
                violations.push(place(expr, 'inline literal in effect deps'));
              } else if (ts.isIdentifier(expr) && unstableVars.has(expr.text)) {
                violations.push(
                  place(
                    expr,
                    `unstable hook return "${expr.text}" from ${unstableVars.get(expr.text)}()`,
                  ),
                );
              }
            }
          }
        }
      }
      ts.forEachChild(node, inspect);
    }
    ts.forEachChild(root, inspect);
  }

  scanScope(source, new Map());
  return violations;
}

it('detects unstable hook-return objects and inline literals in effect dependency arrays', () => {
  const hooks = collectUnstableHooks([
    [
      'useAppNavigation.ts',
      'export function useAppNavigation() { return { bumpBookRevision() {} }; }',
    ],
    [
      'useStable.ts',
      'export function useStable() { return useMemo(() => ({ ok: true }), []); }',
    ],
  ]);
  expect(hooks.has('useAppNavigation')).toBe(true);
  expect(hooks.has('useStable')).toBe(false);

  const bad = findUnstableEffectDeps(
    'App.tsx',
    `
    function App() {
      const nav = useAppNavigation();
      useEffect(() => { nav.bumpBookRevision(); }, [bookKey, nav]);
      useEffect(() => {}, [{ a: 1 }]);
    }
    `,
    hooks,
  );
  expect(bad).toHaveLength(2);
  expect(bad[0]).toContain(
    'unstable hook return "nav" from useAppNavigation()',
  );
  expect(bad[1]).toContain('inline literal in effect deps');

  const good = findUnstableEffectDeps(
    'App.tsx',
    `
    function App() {
      const nav = useAppNavigation();
      const stable = useStable();
      const { bumpBookRevision } = nav;
      useEffect(() => { bumpBookRevision(); }, [bookKey, bumpBookRevision, stable]);
    }
    `,
    hooks,
  );
  expect(good).toEqual([]);
});

it('no component or hook puts an unstable hook return object or inline literal in effect deps', () => {
  const productionFiles = Object.entries(sources).filter(
    ([path]) => !/\.test\.|\.d\.ts$/.test(path),
  );
  const unstableHooks = collectUnstableHooks(productionFiles);
  expect(
    productionFiles.flatMap(([path, text]) =>
      findUnstableEffectDeps(path, text, unstableHooks),
    ),
  ).toEqual([]);
});

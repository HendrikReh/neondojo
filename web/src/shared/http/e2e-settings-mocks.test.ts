import ts from 'typescript';
import { expect, it } from 'vitest';

const sources = import.meta.glob<string>('../../../e2e/**/*.ts', {
  eager: true,
  query: '?raw',
  import: 'default',
});

function hasJsonOption(node: ts.Expression): boolean {
  if (ts.isParenthesizedExpression(node)) return hasJsonOption(node.expression);
  if (ts.isConditionalExpression(node))
    return hasJsonOption(node.whenTrue) || hasJsonOption(node.whenFalse);
  return (
    ts.isObjectLiteralExpression(node) &&
    node.properties.some(
      (p) =>
        (ts.isPropertyAssignment(p) || ts.isShorthandPropertyAssignment(p)) &&
        (ts.isIdentifier(p.name) || ts.isStringLiteral(p.name)) &&
        p.name.text === 'json',
    )
  );
}

function rawSettingsReplies(path: string, text: string) {
  if (!text.replaceAll('\\/', '/').includes('/api/settings')) return [];
  const source = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  const found: string[] = [];
  function visit(node: ts.Node, settingsHandler: boolean) {
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression)
    ) {
      const [first, handler] = node.arguments;
      if (node.expression.name.text === 'route' && first && handler) {
        const matcher = first.getText(source).replaceAll('\\/', '/');
        visit(
          handler,
          /\/api\/settings(?=['"`?$*]|\/[dgimsuvy]*$)/.test(matcher),
        );
        return;
      }
      if (
        settingsHandler &&
        node.expression.name.text === 'fulfill' &&
        first &&
        hasJsonOption(first)
      ) {
        const { line } = source.getLineAndCharacterOfPosition(node.getStart());
        found.push(`${path}:${line + 1}`);
      }
    }
    ts.forEachChild(node, (child) => visit(child, settingsHandler));
  }
  visit(source, false);
  return found;
}

it('finds raw settings JSON while allowing typed replies and transport behavior', () => {
  const text = [
    "page.route('**/api/settings', r => r.fulfill({ json: partial }));",
    "context.route(url => url.pathname === '/api/settings', r => r.fulfill(failed ? { status: 503, json: error } : { json: saved }));",
    String.raw`page.route(/\/api\/settings$/, r => r.fulfill({ status: 409, json }));`,
    "page.route('**/api/settings', r => fulfillJson<'AppSettings'>(r, settings));",
    "page.route('**/api/settings', r => fulfillJson<'ErrorBody'>(r, error, 503));",
    "page.route('**/api/settings', r => r.abort());",
    "page.route('**/api/settings', r => r.fulfill({ body: 'json: unavailable' }));",
    "page.route('**/api/settings', r => r.fallback());",
    "page.route('**/api/settings-other', r => r.fulfill({ json }));",
    "page.route('**/api/settings/detail', r => r.fulfill({ json }));",
    "page.route('**/api/health', r => r.fulfill({ json }));",
    "const response = await page.request.get('/api/settings');",
  ].join('\n');
  expect(rawSettingsReplies('spec', text)).toEqual([
    'spec:1',
    'spec:2',
    'spec:3',
  ]);
});

it('uses typed JSON responses for Playwright settings mocks', () => {
  expect(
    Object.entries(sources).flatMap(([path, text]) =>
      rawSettingsReplies(path, text),
    ),
  ).toEqual([]);
});

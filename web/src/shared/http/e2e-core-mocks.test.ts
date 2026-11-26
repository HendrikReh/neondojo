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

function rawCoreReplies(path: string, text: string) {
  if (
    !text.replaceAll('\\/', '/').match(/\/api\/(settings|engines|play|library)/)
  )
    return [];
  const source = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  const found: string[] = [];
  function visit(node: ts.Node, coreHandler: boolean) {
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression)
    ) {
      const [first, handler] = node.arguments;
      if (node.expression.name.text === 'route' && first && handler) {
        const matcher = first.getText(source).replaceAll('\\/', '/');
        visit(
          handler,
          /\/api\/(?:settings|engines(?:\/bgsage|\/gnubg\/check)?|play\/sessions|library\/matches)(?=['"`?$*]|\/[dgimsuvy]*$)/.test(
            matcher,
          ),
        );
        return;
      }
      if (
        coreHandler &&
        node.expression.name.text === 'fulfill' &&
        first &&
        hasJsonOption(first)
      ) {
        const { line } = source.getLineAndCharacterOfPosition(node.getStart());
        found.push(`${path}:${line + 1}`);
      }
    }
    ts.forEachChild(node, (child) => visit(child, coreHandler));
  }
  visit(source, false);
  return found;
}

it('finds raw core endpoint JSON while allowing typed replies and transport behavior', () => {
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
    "page.route('**/api/engines', r => r.fulfill({ json }));",
    "page.route('**/api/engines/bgsage', r => r.fulfill({ json }));",
    "page.route('**/api/engines/gnubg/check', r => r.fulfill({ json }));",
    "page.route('**/api/engines/other', r => r.fulfill({ json }));",
    "page.route('**/api/engines', r => fulfillJsonList<'EngineInfo'>(r, engines));",
    "page.route('**/api/play/sessions', r => r.fulfill({ json }));",
    "page.route('**/api/library/matches?*', r => r.fulfill({ json }));",
    "page.route(url => url.pathname === '/api/library/matches', r => r.fulfill({ status: 503, json }));",
    "page.route('**/api/play/sessions/*/actions', r => r.fulfill({ json }));",
    "page.route('**/api/library/matches/export', r => r.fulfill({ json }));",
    "page.route('**/api/play/sessions', r => fulfillJsonList<'PlaySession'>(r, sessions));",
    "const response = await page.request.get('/api/settings');",
  ].join('\n');
  expect(rawCoreReplies('spec', text)).toEqual([
    'spec:1',
    'spec:2',
    'spec:3',
    'spec:12',
    'spec:13',
    'spec:14',
    'spec:17',
    'spec:18',
    'spec:19',
  ]);
});

it('uses typed JSON responses for core Playwright API mocks', () => {
  expect(
    Object.entries(sources).flatMap(([path, text]) =>
      rawCoreReplies(path, text),
    ),
  ).toEqual([]);
});

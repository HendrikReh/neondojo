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

const ROUTE_SEGMENT =
  String.raw`(?:\$\{[^}]+\}|[^'"` + '`' + String.raw`?$/]+)`;
const GUARDED_API_PREFILTER =
  /\/api\/(?:settings|engines|play|library|benchmarks|decks|study|matches|jobs|positions)/;
const GUARDED_ROUTE = new RegExp(
  String.raw`\/api\/(?:` +
    [
      'settings',
      String.raw`engines(?:\/bgsage|\/gnubg\/check)?`,
      String.raw`play\/sessions(?:\/${ROUTE_SEGMENT}(?:\/(?:actions|feedback|mode|review))?)?`,
      String.raw`library\/matches`,
      String.raw`benchmarks(?:\/(?:status|reference|play|${ROUTE_SEGMENT}(?:\/cancel)?))?`,
      'decks',
      String.raw`study\/mistakes(?:\/(?:practice|attempt|reveal))?`,
      String.raw`study\/reports(?:\/${ROUTE_SEGMENT})?`,
      String.raw`matches\/${ROUTE_SEGMENT}\/analysis\/estimate`,
      String.raw`jobs\/${ROUTE_SEGMENT}\/remaining-time`,
      String.raw`positions\/${ROUTE_SEGMENT}\/analysis-result`,
    ].join('|') +
    String.raw`)(?=['"` +
    '`' +
    String.raw`?$*\\]|\/[dgimsuvy]*$)`,
);

function rawCoreReplies(path: string, text: string) {
  if (!GUARDED_API_PREFILTER.test(text.replaceAll('\\/', '/'))) return [];
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
        visit(handler, GUARDED_ROUTE.test(matcher));
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
    'page.route(`**/api/play/sessions/${id}/feedback`, r => r.fulfill({ json }));',
    'page.route(`**/api/play/sessions/${id}/mode`, r => r.fulfill({ json }));',
    'page.route(`**/api/play/sessions/${id}/review`, r => r.fulfill({ json }));',
    'page.route(`**/api/play/sessions/${id}/history?*`, r => r.fulfill({ json }));',
    "page.route('**/api/benchmarks', r => r.fulfill({ json }));",
    "page.route('**/api/benchmarks/status', r => r.fulfill({ json }));",
    "page.route('**/api/benchmarks/reference', r => r.fulfill({ json }));",
    "page.route('**/api/benchmarks/play', r => r.fulfill({ json }));",
    'page.route(`**/api/benchmarks/${id}/cancel`, r => r.fulfill({ json }));',
    "page.route('**/api/decks', r => r.fulfill({ json }));",
    'page.route(`**/api/decks/${id}/next`, r => r.fulfill({ json }));',
    String.raw`page.route(/\/api\/study\/mistakes\?/, r => r.fulfill({ json }));`,
    "page.route('**/api/study/mistakes/practice', r => r.fulfill({ json }));",
    "page.route('**/api/study/mistakes/attempt', r => r.fulfill({ json }));",
    "page.route('**/api/study/mistakes/reveal', r => r.fulfill({ json }));",
    "page.route('**/api/study/reports?group_by=session', r => r.fulfill({ json }));",
    'page.route(`**/api/study/reports/${id}`, r => r.fulfill({ json }));',
    'page.route(`**/api/study/reports/${id}/assessment?language=en`, r => r.fulfill({ json }));',
    'page.route(`**/api/matches/${id}/analysis/estimate`, r => r.fulfill({ json }));',
    'page.route(`**/api/jobs/${id}/remaining-time`, r => r.fulfill({ json }));',
    'page.route(`**/api/positions/${id}/analysis-result`, r => r.fulfill({ json }));',
    'page.route(`**/api/matches/${id}/analysis/estimate`, r => fulfillAnalysisEstimate(r, estimate));',
    'page.route(`**/api/positions/${id}/analysis-result`, r => fulfillPositionAnalysisResult(r, null));',
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
    'spec:20',
    'spec:24',
    'spec:25',
    'spec:26',
    'spec:28',
    'spec:29',
    'spec:30',
    'spec:31',
    'spec:32',
    'spec:33',
    'spec:35',
    'spec:36',
    'spec:37',
    'spec:38',
    'spec:39',
    'spec:40',
    'spec:42',
    'spec:43',
    'spec:44',
  ]);
});

it('uses typed JSON responses for core Playwright API mocks', () => {
  expect(
    Object.entries(sources).flatMap(([path, text]) =>
      rawCoreReplies(path, text),
    ),
  ).toEqual([]);
});

import { expect, test } from '@playwright/test';
import type { AnalysisTimeEstimateResult } from '../src/analysis-time-estimate-api';
import { formatAnalysisDuration } from '../src/analysis-time-estimate-messages';

test('uses a completed native benchmark for both analysis scopes without starting work', async ({
  page,
  request,
}) => {
  test.skip(
    process.env.BGTUTOR_NATIVE_ESTIMATE !== '1',
    'requires explicit native estimate opt-in',
  );
  const imported = await request.post('/api/imports', {
    multipart: {
      file: {
        name: 'native-estimate.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          `(;GM[6]PW[Native estimate ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe])`,
        ),
      },
    },
  });
  expect(imported.status()).toBe(201);
  const matchId: string = (await imported.json()).match.id;
  const queued = await request.post('/api/benchmarks', {
    data: { engine_id: 'gnubg', plies: [0] },
  });
  expect(queued.status()).toBe(202);
  const [benchmark] = await queued.json();
  await expect
    .poll(
      async () => {
        const history = await (await request.get('/api/benchmarks')).json();
        return history.runs.find(
          (run: { id: string }) => run.id === benchmark.id,
        )?.status;
      },
      { timeout: 45_000 },
    )
    .toBe('completed');

  const starts: string[] = [];
  page.on('request', (r) => {
    const path = new URL(r.url()).pathname;
    if (
      r.method() === 'POST' &&
      (/\/analysis$/.test(path) ||
        ['/api/benchmarks', '/api/jobs', '/api/imports'].includes(path))
    )
      starts.push(path);
  });
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem(
      'bgtutor.workspace',
      JSON.stringify({ matchId: id, gameNo: 1 }),
    );
  }, matchId);
  await page.goto('/');
  const depth = page.getByLabel('Match analysis depth', { exact: true });
  await expect(depth).toBeEnabled();

  const estimate = page.locator('.analysis-time-estimate');
  for (const [gameNo, moveCount] of [
    [1, 2],
    [null, 3],
  ] as const) {
    const response = page.waitForResponse((r) => {
      const url = new URL(r.url());
      return (
        url.pathname === `/api/matches/${matchId}/analysis/estimate` &&
        url.searchParams.get('plies') === '0' &&
        url.searchParams.get('game_no') ===
          (gameNo === null ? null : String(gameNo))
      );
    });
    if (gameNo === null)
      await page
        .getByLabel('Analysis scope', { exact: true })
        .selectOption('match');
    else await depth.selectOption('0');
    const estimateResponse = await response;
    expect(estimateResponse.ok()).toBe(true);
    const result: AnalysisTimeEstimateResult = await estimateResponse.json();
    expect(result.position_count).toBe(moveCount);
    expect(result.benchmark?.id).toBe(benchmark.id);
    expect(result.estimated_ms).toBeCloseTo(
      moveCount * result.benchmark!.average_move_ms,
      6,
    );
    await expect(estimate).toContainText(
      `Estimated analysis time: approx. ${formatAnalysisDuration(result.estimated_ms!)}`,
    );
    await expect(estimate).toContainText(`${moveCount} checker moves · 0 Ply`);
  }
  expect(starts).toEqual([]);
});

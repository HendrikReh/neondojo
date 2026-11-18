import { openFullMatchAnalysis, openMatchAnalysisOptions } from './analysis-controls';
import { expect, test } from '@playwright/test';
import type { AnalysisTimeEstimateResult } from '../src/analysis-time-estimate-api';
import { formatAnalysisDuration } from '../src/analysis-time-estimate-messages';
import type { components } from '../src/generated-api';

type BenchmarkHistory = components['schemas']['BenchmarkHistory'];
type BenchmarkRun = components['schemas']['BenchmarkRun'];
const depths = [1, 2, 3, 4, 5];
const benchmarkTimeout = 90 * 60_000;

test('uses native benchmarks at every depth for both analysis scopes without starting work', async ({
  page,
  request,
}) => {
  test.skip(
    process.env.BGTUTOR_NATIVE_ESTIMATE !== '1',
    'requires explicit native estimate opt-in',
  );
  test.setTimeout(benchmarkTimeout + 60_000);
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
  const analysisUrl = `/api/matches/${matchId}/analysis`;
  const estimateUrl = `${analysisUrl}/estimate`;
  async function loadRuns() {
    const response = await request.get('/api/benchmarks');
    await expect(response).toBeOK();
    const history: BenchmarkHistory = await response.json();
    return history.runs;
  }

  // The isolated PostgreSQL template can carry completed measurements from
  // this GNUbg installation. Use the estimator to check their compatibility.
  const saved = await loadRuns();
  const benchmarks = new Map<number, BenchmarkRun>();
  for (const plies of depths) {
    if (!saved.some((run) => run.plies === plies && run.status === 'completed'))
      continue;
    const response = await request.get(estimateUrl, { params: { plies } });
    await expect(response).toBeOK();
    const result: AnalysisTimeEstimateResult = await response.json();
    const run = saved.find(
      (entry) =>
        entry.id === result.benchmark?.id &&
        entry.plies === plies &&
        entry.status === 'completed',
    );
    if (run) benchmarks.set(plies, run);
  }

  const missing = depths.filter((plies) => !benchmarks.has(plies));
  if (missing.length) {
    const queued = await request.post('/api/benchmarks', {
      data: { engine_id: 'gnubg', plies: missing },
    });
    expect(queued.status(), await queued.text()).toBe(202);
    const created: BenchmarkRun[] = await queued.json();
    expect(created.map((run) => run.plies)).toEqual(missing);

    let completed: BenchmarkRun[] = [];
    await expect
      .poll(
        async () => {
          const runs = await loadRuns();
          completed = runs.filter((run) =>
            created.some((entry) => entry.id === run.id),
          );
          return (
            completed.some(
              (run) => run.status === 'failed' || run.status === 'cancelled',
            ) ||
            (completed.length === created.length &&
              completed.every((run) => run.status === 'completed'))
          );
        },
        { timeout: benchmarkTimeout, intervals: [500, 1000, 2000, 5000] },
      )
      .toBe(true);
    for (const run of completed) {
      expect(run.status, `${run.plies} Ply: ${run.error ?? ''}`).toBe(
        'completed',
      );
      benchmarks.set(run.plies, run);
    }
  }

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
  await openMatchAnalysisOptions(page);

  const estimate = page.locator('.analysis-time-estimate');
  for (const [gameNo, moveCount] of [
    [1, 2],
    [null, 3],
  ] as const) {
    const controls = gameNo === null ? await openFullMatchAnalysis(page) : page;
    const depth = controls.getByLabel('Match analysis depth', { exact: true });
    await expect(depth).toBeEnabled();
    const initialDepth = Number(await depth.inputValue());
    expect(depths).toContain(initialDepth);
    // Changing the control must trigger a request even for its initial value.
    for (const plies of [
      ...depths.filter((value) => value !== initialDepth),
      initialDepth,
    ]) {
      await test.step(`${gameNo === null ? 'Match' : 'Game'} at ${plies} Ply`, async () => {
        const benchmark = benchmarks.get(plies)!;
        expect(benchmark.average_move_ms).toBeGreaterThan(0);
        const expectedMs = moveCount * benchmark.average_move_ms!;
        const browserRequest = page.waitForRequest((r) => {
          const url = new URL(r.url());
          return (
            r.method() === 'GET' &&
            url.pathname === estimateUrl &&
            url.searchParams.get('engine_id') === 'gnubg' &&
            url.searchParams.get('plies') === String(plies) &&
            url.searchParams.get('game_no') ===
              (gameNo === null ? null : String(gameNo))
          );
        });
        await depth.selectOption(String(plies));
        await browserRequest;
        const estimateResponse = await request.get(estimateUrl, {
          params: {
            plies,
            engine_id: 'gnubg',
            ...(gameNo === null ? {} : { game_no: gameNo }),
          },
        });
        await expect(estimateResponse).toBeOK();
        const result: AnalysisTimeEstimateResult =
          await estimateResponse.json();
        expect(result).toMatchObject({
          match_id: matchId,
          game_no: gameNo,
          plies,
          position_count: moveCount,
          unavailable_reason: null,
          benchmark: {
            id: benchmark.id,
            average_move_ms: benchmark.average_move_ms,
          },
        });
        expect(result.estimated_ms).toBeCloseTo(expectedMs, 6);
        await expect(estimate).toContainText(
          `Estimated analysis time: approx. ${formatAnalysisDuration(expectedMs)}`,
        );
        await expect(estimate).toContainText(
          `${moveCount} checker moves · ${plies} Ply`,
        );
        await expect(
          controls.getByRole('button', {
            name: gameNo === null ? 'Analyze entire match' : 'Analyze game',
            exact: true,
          }),
        ).toBeEnabled();
      });
    }
  }
  for (const suffix of ['', '?game_no=1']) {
    const latest = await request.get(
      `/api/matches/${matchId}/analysis${suffix}`,
    );
    await expect(latest).toBeOK();
    expect(await latest.json()).toBeNull();
  }
  expect(starts).toEqual([]);
});

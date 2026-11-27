import { gotoWorkspace, openWorkspace } from '../helpers/navigation';
import { expect, backendTest as test } from '../helpers/test';

test('native play-turn latency uses the saved playing depth and persists', async ({
  page,
}) => {
  test.skip(
    process.env.NEONDOJO_NATIVE_BENCHMARK !== '1',
    'requires explicit native benchmark opt-in',
  );
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('neondojo.locale', 'en');
  });
  const settings = await (await page.request.get('/api/settings')).json();
  await gotoWorkspace(page, 'extras');
  await page
    .getByRole('button', { name: 'Play-turn latency', exact: true })
    .click();
  const started = page.waitForResponse((response) =>
    response.url().endsWith('/api/benchmarks/play'),
  );
  await page.getByRole('button', { name: 'Measure play-turn latency' }).click();
  const response = await started;
  expect(response.status()).toBe(202);
  const queued = await response.json();
  expect(queued.plies).toBe(settings.play_plies);
  const history = async () => {
    const { runs } = await (await page.request.get('/api/benchmarks')).json();
    return runs.find((run: { id: string }) => run.id === queued.id);
  };
  await expect
    .poll(async () => (await history())?.status, { timeout: 45_000 })
    .toBe('completed');
  const run = await history();
  expect(run.completed_turns).toBe(queued.play_reference.total_turns);
  expect(run.average_turn_ms).toBeGreaterThan(0);
  expect(run.average_move_ms).toBeNull();
  await page.reload();
  await openWorkspace(page, 'extras');
  await page
    .getByRole('button', { name: 'Play-turn latency', exact: true })
    .click();
  const row = page.getByRole('row').filter({ hasText: 'Completed' }).first();
  await expect(row).toContainText('Completed');
  await expect(row.locator('.benchmark-latency')).toHaveText(/\d.* ms/);
});

// Explicit opt-in: creates one retained measurement in the configured backend.
test('native 1-ply benchmark completes and survives a page reload', async ({
  page,
}) => {
  test.skip(
    process.env.NEONDOJO_NATIVE_BENCHMARK !== '1',
    'requires explicit native benchmark opt-in',
  );
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('neondojo.locale', 'en');
  });
  await gotoWorkspace(page, 'extras');
  await expect(
    page.getByRole('heading', { name: 'Engine benchmark' }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Start benchmark', exact: true }),
  ).toBeEnabled();
  for (const ply of [1, 2, 3, 4, 5]) {
    await page
      .getByRole('checkbox', { name: `${ply}-ply`, exact: true })
      .setChecked(ply === 1);
  }
  const started = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/benchmarks') &&
      response.request().method() === 'POST',
  );
  await page
    .getByRole('button', { name: 'Start benchmark', exact: true })
    .click();
  const response = await started;
  expect(response.status()).toBe(202);
  const [created] = await response.json();
  let measured: {
    status: string;
    completed_games: number;
    analyzed_moves: number;
    total_elapsed_ms: number;
    average_move_ms: number;
    error: string | null;
  };
  await expect
    .poll(
      async () => {
        const history = await (
          await page.request.get('/api/benchmarks')
        ).json();
        measured = history.runs.find(
          (run: { id: string }) => run.id === created.id,
        );
        return measured.status;
      },
      { timeout: 45_000 },
    )
    .toBe('completed');
  expect(measured!.completed_games).toBe(10);
  expect(measured!.analyzed_moves).toBe(460);
  expect(measured!.total_elapsed_ms).toBeGreaterThan(0);
  expect(measured!.average_move_ms).toBeCloseTo(
    measured!.total_elapsed_ms / 460,
    6,
  );
  await expect(
    page.locator('.benchmark-history tbody tr').first(),
  ).toContainText('Completed');
  await page.reload();
  await openWorkspace(page, 'extras');
  await expect(
    page.locator('.benchmark-history tbody tr').first(),
  ).toContainText('Completed');
  await expect(
    page.locator('.benchmark-history tbody tr').first(),
  ).toContainText('460 / 460');
  await expect(
    page.getByRole('button', { name: 'Start benchmark', exact: true }),
  ).toBeEnabled();
  const persistedHistory = await (
    await page.request.get('/api/benchmarks')
  ).json();
  const persisted = persistedHistory.runs.find(
    (run: { id: string }) => run.id === created.id,
  );
  expect(persisted).toMatchObject({
    status: 'completed',
    completed_games: 10,
    analyzed_moves: 460,
    total_elapsed_ms: measured!.total_elapsed_ms,
    average_move_ms: measured!.average_move_ms,
    error: null,
  });
  await page.screenshot({
    path: test.info().outputPath('benchmark-native.png'),
    fullPage: true,
  });
  console.log(
    JSON.stringify({
      benchmark_id: created.id,
      total_ms: measured!.total_elapsed_ms,
      average_ms: measured!.average_move_ms,
    }),
  );
});

import { test, expect, type Page } from './test';
import { applySteps } from '../src/play-state';
import { nativeSteps } from '../src/play-sound-events';
import { moneySummary, id } from './study-report-fixture';
const initial = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];
function fixtures() {
  const turns = [
    { player: 0, dice: [3, 1], notation: '8/5 6/5', game: 1 },
    { player: 1, dice: [4, 2], notation: '8/4 6/4', game: 1 },
    { player: 0, dice: [3, 1], notation: '13/10 6/5', game: 1 },
    { player: 0, dice: [6, 1], notation: '13/7 8/7', game: 2 },
  ];
  let board = [...initial];
  const items = turns.map((turn, index) => {
    if (index === 3) board = [...initial];
    const before = {
      id,
      match_uuid: id,
      ordinal: index,
      game_no: turn.game,
      ply: index === 3 ? 1 : index + 1,
      player: turn.player,
      dice: turn.dice,
      board: [...board],
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: turn.notation,
    };
    board = applySteps(
      board,
      nativeSteps(turn.notation, turn.player),
      turn.player,
    );
    return {
      index,
      game_no: turn.game,
      move_no: index + 1,
      kind: 'checker',
      before,
      after: { ...before, board: [...board], dice: [] },
    };
  });
  const decision = (
    number: number,
    checker: number,
    player: number,
    played: string,
    best = played,
    dice = [3, 1],
    kind = 'checker',
  ) => ({
    decision_no: number,
    checker_no: checker,
    player,
    kind,
    played,
    best,
    dice: kind === 'checker' ? dice : null,
    equity_loss: 0.1,
    evaluation_status: 'evaluated',
  });
  const session = {
    id,
    version: 7,
    human_player: 0,
    human_name: 'Learner',
    match_length: 7,
    scores: [7, 3],
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: 'saved engine', plies: 0 },
    phase: 'match_over',
    can_double: false,
    legal_moves: [],
    history: turns.map(({ game, ...turn }) => turn),
    position: items[3].after,
    game_no: 2,
    winner: 0,
    play_mode: 'competition',
    decisions: [],
    review: {
      analysis_plies: 2,
      assisted: false,
      first_attempt_loss: 0.3,
      first_attempt_count: 4,
      retry_count: 0,
      first_attempt_rating: 4.5,
      games: [
        {
          game_no: 1,
          decisions: [
            decision(1, 1, 0, '8/5 6/5', '24/21 6/5'),
            decision(2, 2, 1, '8/4 6/4', '8/4 6/4', [4, 2]),
            decision(3, 3, 0, 'no double', 'double', [], 'cube'),
            decision(4, 3, 0, '13/10 6/5'),
          ],
        },
        {
          game_no: 2,
          decisions: [decision(1, 1, 0, '13/7 8/7', '13/7 8/7', [6, 1])],
        },
      ],
    },
  };
  return { session, history: { session_id: id, version: 7, items } };
}
async function setup(
  page: Page,
  entry: 'study' | 'playing' = 'study',
  reply?: (
    route: import('@playwright/test').Route,
    history: ReturnType<typeof fixtures>['history'],
  ) => Promise<unknown>,
  configure?: (data: ReturnType<typeof fixtures>) => void,
) {
  const data = fixtures();
  configure?.(data);
  let reads = 0;
  const writes: string[] = [];
  page.on('request', (r) => {
    // Personal-profile initialization is independent of reading saved reports.
    if (
      r.method() !== 'GET' &&
      r.url().includes('/api/') &&
      new URL(r.url()).pathname !== '/api/configuration/initialize'
    )
      writes.push(r.url());
  });
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.theme', 'forest');
    localStorage.setItem('bgtutor.play-session', id);
  }, id);
  await page.route('**/api/decks', (r) => r.fulfill({ json: [] }));
  await page.route('**/api/study/reports?group_by=session', (r) =>
    r.fulfill({ json: { items: [moneySummary()], next_cursor: null } }),
  );
  // Study keeps the review dialog for money games; point matches open a
  // dedicated report view.
  await page.route(`**/api/study/reports/${id}`, (r) =>
    r.fulfill({ json: { ...data.session, match_length: 0 } }),
  );
  await page.route(`**/api/play/sessions/${id}`, (r) =>
    r.fulfill({ json: data.session }),
  );
  await page.route(`**/api/play/sessions/${id}/history?*`, (r) => {
    reads++;
    expect(new URL(r.request().url()).searchParams.get('version')).toBe('7');
    return reply ? reply(r, data.history) : r.fulfill({ json: data.history });
  });
  await page.goto('/');
  if (entry === 'study') {
    await page.getByRole('tab', { name: 'Study', exact: true }).click();
    await page.getByRole('tab', { name: 'Reports', exact: true }).click();
    await page
      .getByRole('button', { name: 'Open report', exact: true })
      .click();
  } else {
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page
      .getByRole('button', { name: 'Back to board', exact: true })
      .click();
    await page
      .getByRole('button', { name: 'Learning review', exact: true })
      .click();
  }
  const dialog = page.getByRole('dialog', {
    name: 'Learning review',
    exact: true,
  });
  await expect(dialog).toBeVisible();
  return { dialog, reads: () => reads, writes };
}
for (const entry of ['study', 'playing'] as const)
  test(`${entry} decision selection previews its best move from the correct starting board`, async ({
    page,
  }) => {
    const { dialog, reads, writes } = await setup(page, entry);
    expect(reads()).toBe(0);
    await dialog.locator('tbody tr').first().click();
    const preview = dialog.getByRole('region', {
      name: 'Decision preview',
      exact: true,
    });
    await expect(preview.locator('.move-arrows')).toBeVisible();
    await expect(preview).toContainText('24/21 6/5');
    await expect(dialog.locator('tbody tr').first()).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await preview.getByRole('button', { name: 'Final', exact: true }).click();
    await expect(preview.locator('[data-point="21"]').first()).toHaveAttribute(
      'data-count',
      '1',
    );
    await expect(preview.locator('[data-point="8"]').first()).toHaveAttribute(
      'data-count',
      '3',
    );
    await preview.getByRole('button', { name: 'Initial', exact: true }).click();
    await expect(preview.locator('[data-point="24"]').first()).toHaveAttribute(
      'data-count',
      '2',
    );
    const later = dialog.locator('tbody tr').nth(2);
    await later.click();
    await expect(preview).toContainText('13/10 6/5');
    await expect(preview.locator('[data-point="8"]').first()).toHaveAttribute(
      'data-count',
      '2',
    );
    await dialog.locator('tbody tr').last().click();
    await expect(preview).toContainText('13/7 8/7');
    await expect(preview.locator('[data-point="8"]').first()).toHaveAttribute(
      'data-count',
      '3',
    );
    expect(reads()).toBe(1);
    expect(writes).toEqual([]);
    await page.screenshot({
      path: test.info().outputPath(`${entry}-decision-preview.png`),
    });
  });
test('keyboard selection supports cube decisions without invented checker arrows', async ({
  page,
}) => {
  const { dialog } = await setup(page);
  const first = dialog.locator('tbody tr').first();
  await first.focus();
  await first.press('Enter');
  await first.press('ArrowDown');
  const cube = dialog.locator('tbody tr').nth(1);
  await expect(cube).toBeFocused();
  await expect(cube).toHaveAttribute('aria-selected', 'true');
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview).toContainText('Double');
  await expect(preview.locator('.board')).toBeVisible();
  await expect(preview.locator('.move-arrows')).toHaveCount(0);
  await expect(
    preview.getByRole('button', { name: 'Final', exact: true }),
  ).toBeDisabled();
  await cube.press('End');
  await expect(dialog.locator('tbody tr').last()).toBeFocused();
  await expect(preview).toContainText('13/7 8/7');
});
test('history errors support retry without writing to the saved match', async ({
  page,
}) => {
  let fail = true;
  const { dialog, writes } = await setup(page, 'study', async (r, h) =>
    fail
      ? r.fulfill({ status: 503, json: { error: 'Unavailable' } })
      : r.fulfill({ json: h }),
  );
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview.getByRole('alert')).toBeVisible();
  fail = false;
  await preview.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(preview.locator('.move-arrows')).toBeVisible();
  expect(writes).toEqual([]);
});
for (const width of [390, 1440])
  test(`selected preview and decision list remain reachable at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    const { dialog } = await setup(page);
    await dialog.locator('tbody tr').first().click();
    const preview = dialog.getByRole('region', {
      name: 'Decision preview',
      exact: true,
    });
    await expect(preview.locator('.board')).toBeVisible();
    const box = (await dialog.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(15);
    expect(box.x + box.width).toBeLessThanOrEqual(width - 15);
    await expect(
      dialog.getByRole('button', { name: 'Close', exact: true }),
    ).toBeInViewport();
    await dialog.locator('tbody tr').last().focus();
    await dialog.locator('tbody tr').last().press('Enter');
    await expect(preview).toContainText('13/7 8/7');
    await page.screenshot({
      path: test.info().outputPath(`preview-${width}.png`),
    });
  });

test('invalid best notation preserves the source board and disables unsafe projection', async ({
  page,
}) => {
  const { dialog } = await setup(page, 'study', undefined, (data) => {
    data.session.review.games[0].decisions[0].best = 'not a legal move';
  });
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview.locator('.board')).toBeVisible();
  await expect(preview.locator('[data-point="24"]').first()).toHaveAttribute(
    'data-count',
    '2',
  );
  await expect(
    preview.getByRole('button', { name: 'Final', exact: true }),
  ).toBeDisabled();
  await expect(preview).toContainText('cannot be previewed safely');
});
test('Black uses its own original board and player-relative best-move notation', async ({
  page,
}) => {
  const { dialog } = await setup(page, 'study', undefined, (data) => {
    data.session.human_player = 1;
  });
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview.locator('.move-arrows')).toBeVisible();
  await preview.getByRole('button', { name: 'Final', exact: true }).click();
  await expect(preview.locator('[data-point="21"]').first()).toHaveAttribute(
    'data-count',
    '-2',
  );
});
test('changing the selection while history loads shows only the latest selected decision', async ({
  page,
}) => {
  let release = () => {};
  const delayed = new Promise<void>((resolve) => {
    release = resolve;
  });
  const { dialog } = await setup(page, 'study', async (r, h) => {
    await delayed;
    await r.fulfill({ json: h });
  });
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview.getByRole('status')).toContainText('Loading position');
  await dialog.locator('tbody tr').last().click();
  release();
  await expect(preview).toContainText('13/7 8/7');
  await expect(preview.locator('.move-arrows')).toBeVisible();
  await expect(preview.locator('[data-point="8"]').first()).toHaveAttribute(
    'data-count',
    '3',
  );
});
test('wrong-version history is rejected and can be retried safely', async ({
  page,
}) => {
  let correct = false;
  const { dialog } = await setup(page, 'study', (r, h) =>
    r.fulfill({ json: { ...h, version: correct ? 7 : 8 } }),
  );
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview.getByRole('alert')).toBeVisible();
  await expect(preview.locator('.board')).toHaveCount(0);
  correct = true;
  await preview.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(preview.locator('.board')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});

test('a decision chosen below a long mobile list brings its updated board into view', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { dialog } = await setup(page, 'study', undefined, (data) => {
    const source = data.history.items[3];
    for (let game = 3; game <= 35; game++) {
      const index = data.history.items.length;
      data.history.items.push({
        ...structuredClone(source),
        index,
        game_no: game,
        before: {
          ...structuredClone(source.before),
          game_no: game,
          ordinal: index,
        },
      });
      data.session.history.push({ ...data.session.history[3] });
      data.session.review.games.push({
        game_no: game,
        decisions: [{ ...data.session.review.games[1].decisions[0] }],
      });
    }
  });
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview.locator('.board')).toBeVisible();
  const last = dialog.locator('tbody tr').last();
  await last.click();
  await expect(preview.locator('.board')).toBeInViewport({ ratio: 0.9 });
  await last.press('ArrowUp');
  await expect(preview.locator('.board')).toBeInViewport({ ratio: 0.9 });
});

test('closing while history loads discards the late preview response', async ({
  page,
}) => {
  let release = () => {};
  const delayed = new Promise<void>((resolve) => {
    release = resolve;
  });
  const { dialog } = await setup(page, 'study', async (r, h) => {
    await delayed;
    await r.fulfill({ json: h }).catch(() => {});
  });
  await dialog.locator('tbody tr').first().click();
  await expect(dialog.getByRole('status')).toContainText('Loading position');
  await page.keyboard.press('Escape');
  release();
  await expect(dialog).toHaveCount(0);
  await page.getByRole('button', { name: 'Open report', exact: true }).click();
  await expect(
    dialog.getByRole('region', { name: 'Decision preview', exact: true }),
  ).toHaveCount(0);
});
test('a missing decision link never substitutes the final match board', async ({
  page,
}) => {
  const { dialog } = await setup(page, 'study', undefined, (data) => {
    data.session.review.games[0].decisions[0].checker_no = 0;
  });
  await dialog.locator('tbody tr').first().click();
  const preview = dialog.getByRole('region', {
    name: 'Decision preview',
    exact: true,
  });
  await expect(preview).toContainText(
    'saved position for this decision is unavailable',
  );
  await expect(preview.locator('.board')).toHaveCount(0);
});

test('an archived money report uses its own immutable history rather than the advanced live session', async ({
  page,
}) => {
  const liveId = '5bfa0ecb-2a4d-432b-8ac5-b9019ac69eb8';
  let savedHistory: ReturnType<typeof fixtures>['history'];
  let archiveReads = 0;
  await page.route(`**/api/study/reports/${id}/history?*`, (route) => {
    archiveReads++;
    expect(new URL(route.request().url()).searchParams.get('version')).toBe(
      '7',
    );
    return route.fulfill({ json: savedHistory });
  });
  const { dialog, reads } = await setup(page, 'study', undefined, (data) => {
    Object.assign(data.session, {
      id: liveId,
      study_report_id: id,
      study_report_version: 7,
      review_game_no: 1,
      match_length: 0,
    });
    data.history.session_id = liveId;
    for (const event of data.history.items) {
      event.before.match_uuid = liveId;
      event.after.match_uuid = liveId;
    }
    savedHistory = data.history;
  });
  await expect(dialog.locator('.play-learning-review')).toBeVisible();
  await dialog.locator('tbody tr').first().click();
  await expect(
    dialog
      .getByRole('region', { name: 'Decision preview', exact: true })
      .locator('.move-arrows'),
  ).toBeVisible();
  expect(archiveReads).toBe(1);
  expect(reads()).toBe(0);
});

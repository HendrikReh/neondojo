import { expect, test, type Page } from './test';

test.use({ actionTimeout: 10000, viewport: { width: 1800, height: 1169 } });

async function initialize(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.theme', 'forest');
  });
  await page.goto('/');
}
async function imported(page: Page) {
  const white = `Plans ${crypto.randomUUID()}`,
    black = 'Black plan player';
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: `plans-${crypto.randomUUID()}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[${white}]PB[${black}]MI[length:7];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return { white, black };
}
async function displayedBoard(page: Page) {
  return page
    .locator('.analysis-workspace .board [data-point][data-count]')
    .evaluateAll((checkers) => {
      const board = Array<number>(26).fill(0);
      for (const checker of checkers)
        board[Number(checker.getAttribute('data-point'))] = Number(
          checker.getAttribute('data-count'),
        );
      return board;
    });
}
function gate() {
  let release!: () => void;
  const waiting = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { waiting, release };
}
const response = (white: string, black: string, game_over = false) => ({
  classifier: 'opensage_rules_v1_opening1',
  source_revision: '139427d826dd67798226e7d23c003c90315306ba',
  game_over,
  white,
  black,
});

test('real import shows both named plans without analysis and follows the displayed board through stepping and views', async ({
  page,
}) => {
  const requests: number[][] = [],
    jobs: string[] = [];
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (path === '/api/game-plans') requests.push(request.postDataJSON().board);
    if (path === '/api/jobs' && request.method() === 'POST') jobs.push(path);
  });
  await initialize(page);
  const names = await imported(page);
  const panel = page.getByRole('region', { name: 'Game plan', exact: true });
  await expect(panel.locator('[data-game-plan]')).toHaveCount(2);
  await expect(panel.locator('[data-game-plan="white"]')).toContainText(
    names.white,
  );
  await expect(panel.locator('[data-game-plan="black"]')).toContainText(
    names.black,
  );
  await panel.locator('.game-plan-help').focus();
  await expect(page.getByRole('tooltip')).toContainText(
    /Open Sage.*board structure.*GNUbg/,
  );
  const initial = await displayedBoard(page);
  expect(requests.at(-1)).toEqual(initial);
  const initialCount = requests.length;
  await page.getByRole('button', { name: 'Initial', exact: true }).click();
  await expect(panel.locator('[data-game-plan]')).toHaveCount(2);
  expect(requests.length).toBe(initialCount);
  await page.getByRole('button', { name: 'Final', exact: true }).click();
  await expect(panel.locator('[data-game-plan]')).toHaveCount(2);
  expect(requests.at(-1)).toEqual(await displayedBoard(page));
  expect(requests.at(-1)).not.toEqual(initial);
  await page.getByRole('button', { name: 'Arrows', exact: true }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-heading')).toContainText(names.black);
  await expect(panel.locator('[data-game-plan="white"]')).toContainText(
    names.white,
  );
  await expect(panel.locator('[data-game-plan="black"]')).toContainText(
    names.black,
  );
  await page
    .getByRole('button', { name: 'Previous move', exact: true })
    .click();
  await page.getByRole('button', { name: 'Initial', exact: true }).click();
  await expect(panel.locator('[data-game-plan]')).toHaveCount(2);
  expect(await displayedBoard(page)).toEqual(initial);
  expect(jobs).toEqual([]);
});

test('late board responses cannot replace a newer Initial selection after Final and stepping', async ({
  page,
}) => {
  await initialize(page);
  const initialGate = gate(),
    finalGate = gate();
  let initialBoard: number[] | undefined;
  let finalBoard: number[] | undefined;
  let returnedBoard: number[] | undefined;
  let returning = false;
  let delayed = 0;
  let finished = 0;
  await page.route('**/api/game-plans', async (route) => {
    const { board } = route.request().postDataJSON() as { board: number[] };
    initialBoard ??= board;
    const isInitial = board.every(
      (count, point) => count === initialBoard![point],
    );
    if (isInitial && returning) {
      returnedBoard = board;
      await route.fulfill({ json: response('anchoring', 'racing') });
      return;
    }
    // React may cancel and repeat a request for the same board on mount.
    // Hold every response for the old selection, regardless of request count.
    delayed++;
    if (!isInitial) finalBoard = board;
    await (isInitial ? initialGate.waiting : finalGate.waiting);
    const result = isInitial
      ? response('attacking', 'priming')
      : response('priming', 'attacking');
    await route.fulfill({ json: result }).catch(() => {});
    finished++;
  });
  const panel = page.getByRole('region', { name: 'Game plan', exact: true });
  const names = await imported(page);
  try {
    await expect.poll(() => initialBoard).toEqual(await displayedBoard(page));
    await expect(panel).toContainText('Loading game plan');
    await expect(panel.locator('[data-game-plan]')).toHaveCount(0);
    await page.getByRole('button', { name: 'Final', exact: true }).click();
    await expect.poll(() => finalBoard).toEqual(await displayedBoard(page));
    expect(finalBoard).not.toEqual(initialBoard);
    await expect(panel.locator('[data-game-plan]')).toHaveCount(0);
    returning = true;
    await page.getByRole('button', { name: 'Initial', exact: true }).click();
    await expect.poll(() => returnedBoard).toEqual(initialBoard);
    await expect(panel.locator('[data-game-plan="white"]')).toContainText(
      'Anchoring',
    );
    await expect(panel.locator('[data-game-plan="black"]')).toContainText(
      'Racing (contact)',
    );
  } finally {
    initialGate.release();
    finalGate.release();
  }
  await expect.poll(() => finished === delayed).toBe(true);
  await expect(panel.locator('[data-game-plan="white"]')).toContainText(
    'Anchoring',
  );
  await expect(panel.locator('[data-game-plan="black"]')).toContainText(
    'Racing (contact)',
  );
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(panel.locator('[data-game-plan="white"]')).toContainText(
    names.white,
  );
  await expect(panel.locator('[data-game-plan="black"]')).toContainText(
    names.black,
  );
});

test('an unavailable plan leaves navigation usable and can be retried', async ({
  page,
}) => {
  await initialize(page);
  let available = false;
  await page.route('**/api/game-plans', (route) =>
    route.fulfill(
      available
        ? { json: response('pure_race', 'pure_race', true) }
        : { status: 503, json: { error: 'Unavailable' } },
    ),
  );
  await imported(page);
  const panel = page.getByRole('region', { name: 'Game plan', exact: true });
  await expect(panel).toContainText('Game plan unavailable');
  await expect(
    page.getByRole('button', { name: 'Next move', exact: true }),
  ).toBeEnabled();
  available = true;
  await panel.getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(panel).toContainText('Game finished');
  await expect(panel).not.toContainText('Pure race');
});

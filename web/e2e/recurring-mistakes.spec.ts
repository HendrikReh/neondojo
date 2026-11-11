import { test, expect, type Page } from './test';
import type { Database } from '../src/library-api';

const database: Database = {
  id: '00000000-0000-4000-8000-000000000003',
  name: 'Club matches',
  description: '',
  is_default: false,
  is_system: false,
  match_count: 2,
  created_at: '2026-11-10T12:00:00Z',
  updated_at: '2026-11-10T12:00:00Z',
  version: 1,
};
const position = {
  id: 'position',
  match_uuid: 'match',
  ordinal: 1,
  game_no: 1,
  ply: 1,
  player: 0,
  dice: [3, 1],
  board: [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ],
  position_id: '4HPwATDgc/ABMA',
  gnubg_match_id: 'cIkAAAAAAAAA',
  notation: '',
};
const question = (n: number) => ({
  reference: { job_id: 'job', game_no: 1, decision_no: n },
  kind: 'checker',
  position: { ...position, id: `position-${n}` },
  players: ['Alice', 'Bob'],
  engine: 'gnubg',
  plies: 2,
});
const group = {
  game_plan: 'priming',
  mistake_kind: 'checker',
  count: 3,
  match_count: 2,
  total_loss: 0.36,
};
const result = {
  players: ['Alice', 'Bob'],
  groups: [group],
  analyzed_decisions: 22,
  analyzed_matches: 2,
};
const answer = {
  best: '13/10 6/5',
  played: '8/5 6/5',
  equity_loss: 0.12,
  engine: 'gnubg',
  plies: 2,
};
async function open(page: Page, locale = 'en') {
  await page.addInitScript(
    (locale) => localStorage.setItem('bgtutor.locale', locale),
    locale,
  );
  await page.route('**/api/decks', (r) => r.fulfill({ json: [] }));
  await page.route('**/api/databases', (r) => r.fulfill({ json: [database] }));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', {
      name:
        locale === 'de'
          ? 'Meine wiederkehrenden Fehler'
          : 'My recurring mistakes',
      exact: true,
    })
    .click();
}
async function list(page: Page) {
  await page.route('**/api/study/mistakes?*', (r) =>
    r.fulfill({
      json: {
        ...result,
        groups: new URL(r.request().url()).searchParams.has('player')
          ? [group]
          : [],
      },
    }),
  );
}
test('imported practice keeps original depth, uses imported references and hides the next source answer', async ({
  page,
}) => {
  await list(page);
  const importedQuestion = (
    id: string,
    engine: string,
    source: string,
    depth: string | null,
  ) => ({
    ...question(1),
    reference: { imported_analysis_id: id, game_no: 1, decision_no: 1 },
    engine,
    source,
    depth,
    plies: null,
    method: source === 'xg' ? 'rollout' : null,
  });
  await page.route('**/api/study/mistakes/practice', (r) =>
    r.fulfill({
      json: {
        items: [
          importedQuestion('xg-record', 'eXtreme Gammon', 'xg', 'XGRoller++'),
          importedQuestion('bgf-record', 'BGBlitz source', 'bgblitz', null),
        ],
      },
    }),
  );
  const references: unknown[] = [];
  await page.route('**/api/study/mistakes/reveal', (r) => {
    references.push(r.request().postDataJSON());
    return r.fulfill({
      json: {
        ...answer,
        engine: 'eXtreme Gammon',
        source: 'xg',
        depth: 'XGRoller++',
        method: 'rollout',
        plies: null,
      },
    });
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  await expect(panel).toContainText('XG');
  await expect(panel).toContainText('BGBlitz');
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await panel.getByRole('button', { name: 'Practice', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await expect(dialog).toContainText(
    'Imported XG · eXtreme Gammon · XGRoller++ · rollout',
  );
  await expect(dialog).not.toContainText(answer.best);
  await dialog
    .getByRole('button', { name: 'Reveal answer', exact: true })
    .click();
  await expect(dialog).toContainText(answer.best);
  expect(references).toEqual([
    { imported_analysis_id: 'xg-record', game_no: 1, decision_no: 1 },
  ]);
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(dialog).toContainText(
    'Imported BGBlitz · BGBlitz source · Unknown depth',
  );
  await expect(dialog).not.toContainText(answer.best);
  await expect(dialog).not.toContainText('0 ply');
  await expect(
    dialog.getByRole('button', { name: 'Reveal answer', exact: true }),
  ).toBeVisible();
});
test('requires an exact player, filters groups and reveals saved answers only on demand, resets on Next and completes', async ({
  page,
}) => {
  const writes: string[] = [];
  page.on('request', (r) => {
    const p = new URL(r.url()).pathname;
    if (
      r.method() !== 'GET' &&
      p.startsWith('/api/') &&
      !/^\/api\/(configuration|profiles|study\/mistakes)(\/|$)/.test(p)
    )
      writes.push(p);
  });
  await list(page);
  let practiceBody: unknown;
  await page.route('**/api/study/mistakes/practice', (r) => {
    practiceBody = r.request().postDataJSON();
    return r.fulfill({ json: { items: [question(1), question(2)] } });
  });
  let reveals = 0;
  await page.route('**/api/study/mistakes/reveal', (r) => {
    reveals++;
    return r.fulfill({ json: answer });
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  await expect(panel).toContainText('Choose a player');
  await expect(
    panel.getByRole('button', { name: 'Practice', exact: true }),
  ).toHaveCount(0);
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await panel
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(database.id);
  await panel
    .getByRole('combobox', { name: 'Game plan', exact: true })
    .selectOption('priming');
  await expect(panel).toContainText('0.360');
  await page.screenshot({
    path: test.info().outputPath('recurring-mistakes-dashboard.png'),
    fullPage: true,
  });
  const trigger = panel.getByRole('button', { name: 'Practice', exact: true });
  await trigger.click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await expect(dialog).toContainText('1 / 2');
  await expect(dialog).toContainText('gnubg');
  await expect(dialog.getByRole('img')).toBeVisible();
  await expect(dialog).not.toContainText(answer.best);
  await expect(dialog).not.toContainText(answer.played);
  expect(reveals).toBe(0);
  await expect(dialog.locator('.move-arrows')).toHaveCount(0);
  await page.screenshot({
    path: test.info().outputPath('recurring-mistakes-question.png'),
    fullPage: true,
  });
  expect(practiceBody).toEqual({
    player: 'Alice',
    database_id: database.id,
    game_plan: 'priming',
    min_loss: 0.08,
    mistake_kind: 'checker',
  });
  await dialog
    .getByRole('button', { name: 'Reveal answer', exact: true })
    .click();
  await expect(dialog).toContainText(answer.best);
  await expect(dialog).toContainText(answer.played);
  await expect(dialog).toContainText('0.120');
  await page.screenshot({
    path: test.info().outputPath('recurring-mistakes-answer.png'),
    fullPage: true,
  });
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(dialog).toContainText('2 / 2');
  await expect(dialog).not.toContainText(answer.best);
  await expect(dialog).not.toContainText(answer.played);
  await dialog
    .getByRole('button', { name: 'Reveal answer', exact: true })
    .click();
  await dialog.getByRole('button', { name: 'Finish', exact: true }).click();
  await expect(dialog).toContainText('Practice complete');
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(trigger).toBeFocused();
  expect(writes).toEqual([]);
});
test('list, practice and reveal support retry; closing cancels a late answer and reopening starts hidden', async ({
  page,
}) => {
  let failList = true,
    failPractice = true,
    failReveal = true;
  await page.route('**/api/study/mistakes?*', (r) =>
    failList
      ? r.fulfill({ status: 503, json: { error: 'Unavailable' } })
      : r.fulfill({ json: result }),
  );
  await page.route('**/api/study/mistakes/practice', (r) =>
    failPractice
      ? r.fulfill({ status: 503, json: { error: 'Unavailable' } })
      : r.fulfill({ json: { items: [question(1), question(2)] } }),
  );
  let release = () => {};
  let delayed = false;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/study/mistakes/reveal', async (r) => {
    if (delayed) await pending;
    await (
      failReveal
        ? r.fulfill({ status: 503, json: { error: 'Unavailable' } })
        : r.fulfill({ json: answer })
    ).catch(() => {});
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  await expect(panel.getByRole('alert')).toBeVisible();
  failList = false;
  await panel.getByRole('button', { name: 'Try again', exact: true }).click();
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await panel.getByRole('button', { name: 'Practice', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await expect(dialog.getByRole('alert')).toBeVisible();
  failPractice = false;
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await dialog
    .getByRole('button', { name: 'Reveal answer', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  failReveal = false;
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(dialog).toContainText(answer.best);
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  delayed = true;
  await dialog
    .getByRole('button', { name: 'Reveal answer', exact: true })
    .click();
  await page.keyboard.press('Escape');
  await panel.getByRole('button', { name: 'Practice', exact: true }).click();
  release();
  await expect(dialog).toContainText('1 / 2');
  await expect(dialog).not.toContainText(answer.best);
  await expect(
    dialog.getByRole('button', { name: 'Reveal answer', exact: true }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('tab', { name: 'Reports', exact: true }).click();
  await expect(dialog).toBeHidden();
});
test('late lists cannot replace a newer player and empty groups are honest', async ({
  page,
}) => {
  let release = () => {};
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/study/mistakes?*', async (r) => {
    const player = new URL(r.request().url()).searchParams.get('player');
    if (player === 'Alice') await pending;
    await r
      .fulfill({
        json: { ...result, groups: player === 'Alice' ? [group] : [] },
      })
      .catch(() => {});
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Bob');
  await expect(panel).toContainText(
    'No recurring mistakes match these filters.',
  );
  release();
  await expect(
    panel.getByRole('button', { name: 'Practice', exact: true }),
  ).toHaveCount(0);
});

test('Next ignores a late reveal and leaving Study cancels pending practice', async ({
  page,
}) => {
  await list(page);
  let delayPractice = false,
    releasePractice = () => {},
    releaseReveal = () => {};
  const pendingPractice = new Promise<void>((resolve) => {
    releasePractice = resolve;
  });
  const pendingReveal = new Promise<void>((resolve) => {
    releaseReveal = resolve;
  });
  await page.route('**/api/study/mistakes/practice', async (r) => {
    const delayed = delayPractice;
    if (delayed) await pendingPractice;
    await r
      .fulfill({
        json: {
          items: delayed ? [question(99)] : [question(1), question(2)],
        },
      })
      .catch(() => {});
  });
  await page.route('**/api/study/mistakes/reveal', async (r) => {
    await pendingReveal;
    await r.fulfill({ json: answer }).catch(() => {});
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  const trigger = panel.getByRole('button', { name: 'Practice', exact: true });
  await trigger.click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await dialog
    .getByRole('button', { name: 'Reveal answer', exact: true })
    .click();
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  releaseReveal();
  await expect(dialog).toContainText('2 / 2');
  await expect(dialog).not.toContainText(answer.best);
  await page.keyboard.press('Escape');
  delayPractice = true;
  await trigger.click();
  await expect(dialog).toContainText('Loading practice');
  // External navigation (such as a deck request) can change visibility while a
  // modal is active; dispatch activates the same tab handler despite inert UI.
  await page
    .getByRole('tab', { name: 'Reports', exact: true })
    .dispatchEvent('click');
  await expect(dialog).toBeHidden();
  delayPractice = false;
  await page
    .getByRole('tab', { name: 'My recurring mistakes', exact: true })
    .click();
  await trigger.click();
  await expect(dialog).toContainText('1 / 2');
  releasePractice();
  await expect(dialog).not.toContainText('1.99');
  await expect(dialog).toContainText('1 / 2');
});

test('position search exposes the same game plan labels and sends the chosen optional filter', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.route('**/api/decks', (r) => r.fulfill({ json: [] }));
  await page.route('**/api/databases', (r) => r.fulfill({ json: [] }));
  await page.route('**/api/position-filters', (r) => r.fulfill({ json: [] }));
  const definitions: Record<string, unknown>[] = [];
  await page.route('**/api/position-browser?*', (r) => {
    definitions.push(
      JSON.parse(new URL(r.request().url()).searchParams.get('filters')!),
    );
    return r.fulfill({ json: { items: [], next_cursor: null } });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const pane = page.getByRole('tabpanel', { name: 'Positions', exact: true });
  await pane.locator('.position-filter-details summary').click();
  const select = pane.getByRole('combobox', { name: 'Game plan', exact: true });
  await expect(select.getByRole('option')).toHaveText([
    'Any',
    'Pure race',
    'Racing (contact)',
    'Attacking / blitz',
    'Priming',
    'Anchoring',
  ]);
  await select.selectOption('priming');
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect.poll(() => definitions.at(-1)?.game_plan).toBe('priming');
  await select.selectOption('');
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect.poll(() => definitions.at(-1)?.game_plan).toBeUndefined();
});

for (const [locale, title] of [
  ['en', 'My recurring mistakes'],
  ['de', 'Meine wiederkehrenden Fehler'],
] as const) {
  test(`practice remains keyboard accessible within a narrow viewport (${locale})`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await list(page);
    await page.route('**/api/study/mistakes/practice', (r) =>
      r.fulfill({ json: { items: [question(1)] } }),
    );
    await open(page, locale);
    const panel = page.getByRole('region', { name: title, exact: true });
    await panel
      .getByRole('combobox', {
        name: locale === 'en' ? 'Player' : 'Spieler',
        exact: true,
      })
      .selectOption('Alice');
    const trigger = panel.getByRole('button', {
      name: locale === 'en' ? 'Practice' : 'Üben',
      exact: true,
    });
    await trigger.focus();
    await trigger.press('Enter');
    const dialog = page.getByRole('dialog');
    const heading = dialog.locator('h2');
    await expect(heading).toBeFocused();
    await expect(heading).toBeInViewport();
    const box = await dialog.boundingBox();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(390);
    await page.keyboard.press('Shift+Tab');
    await expect(
      dialog
        .getByRole('button', {
          name: locale === 'en' ? 'Close' : 'Schließen',
          exact: true,
        })
        .last(),
    ).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(
      dialog
        .getByRole('button', {
          name: locale === 'en' ? 'Close' : 'Schließen',
          exact: true,
        })
        .first(),
    ).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();
  });
}

test('a player unavailable in the selected database resets explicitly, and invalid thresholds do not request data', async ({
  page,
}) => {
  const calls: URL[] = [];
  await page.route('**/api/study/mistakes?*', (r) => {
    const url = new URL(r.request().url());
    calls.push(url);
    return r.fulfill({
      json: url.searchParams.has('database_id')
        ? {
            players: ['Bob'],
            groups: [],
            analyzed_decisions: 0,
            analyzed_matches: 0,
          }
        : { ...result, groups: url.searchParams.has('player') ? [group] : [] },
    });
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  const player = panel.getByRole('combobox', { name: 'Player', exact: true });
  await player.selectOption('Alice');
  await expect(
    panel.getByRole('button', { name: 'Practice', exact: true }),
  ).toBeVisible();
  await panel
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(database.id);
  await expect(player).toHaveValue('');
  await expect(panel).toContainText(
    'Choose a player to see their recurring mistakes.',
  );
  await expect.poll(() => calls.at(-1)?.searchParams.has('player')).toBe(false);
  await panel
    .getByRole('spinbutton', { name: 'Minimum equity loss', exact: true })
    .fill('1001');
  await expect(panel.getByRole('alert')).toContainText('1000');
  expect(
    calls.every((url) => Number(url.searchParams.get('min_loss')) <= 1000),
  ).toBe(true);
});

test('practice shows original match length and money-game Jacoby rules before revealing any answer', async ({
  page,
}) => {
  await list(page);
  // GNUbg Match IDs encode 7-point play, money with Jacoby and money without
  // Jacoby respectively. Board scores alone cannot distinguish these contexts.
  const contexts = ['cIngAAAAAAAE', 'cIkAAAAAAAAA', 'cIkAAAAAAAAE'];
  await page.route('**/api/study/mistakes/practice', (route) =>
    route.fulfill({
      json: {
        items: contexts.map((gnubg_match_id, index) => ({
          ...question(index + 1),
          kind: 'cube_offer',
          position: { ...position, gnubg_match_id, dice: [] },
        })),
      },
    }),
  );
  let reveals = 0;
  await page.route('**/api/study/mistakes/reveal', (route) => {
    reveals++;
    return route.fulfill({ json: answer });
  });
  await open(page);
  const panel = page.getByRole('region', {
    name: 'My recurring mistakes',
    exact: true,
  });
  await panel
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await panel.getByRole('button', { name: 'Practice', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await expect(dialog).toContainText('7-point match');
  await expect(dialog).not.toContainText('Jacoby');
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(dialog).toContainText('Money game · Jacoby ✓');
  await expect(dialog).not.toContainText('7-point match');
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(dialog).toContainText('Money game · Jacoby ✗');
  await expect(dialog).not.toContainText(answer.best);
  await expect(dialog).not.toContainText(answer.played);
  expect(reveals).toBe(0);
});

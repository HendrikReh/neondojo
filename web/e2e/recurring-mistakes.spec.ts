import { test, expect, type Page } from './test';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/settings/configuration/configuration-model';
import type { Database } from '../src/workspaces/library/library-api';

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
  plies: 3,
});
const group = {
  game_plan: 'priming',
  mistake_kind: 'checker',
  count: 3,
  match_count: 2,
  total_loss: 0.36,
  rates: {
    decision_count: 6,
    error_rate: 0.5,
    loss_per_100: 6,
    small_sample: true,
  },
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
  plies: 3,
};
async function open(page: Page, locale = 'en') {
  await page.addInitScript(
    (locale) => localStorage.setItem('neondojo.locale', locale),
    locale,
  );
  await page.route('**/api/decks', (r) => r.fulfill({ json: [] }));
  await page.route('**/api/databases', (r) => r.fulfill({ json: [database] }));
  await page.goto('/');
  await page
    .getByRole('tab', {
      name: locale === 'de' ? 'Lernen' : 'Study',
      exact: true,
    })
    .click();
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

test('checks checker moves, cube offers and cube replies in the practice dialog', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await list(page);
  const cases = [
    {
      item: question(1),
      submitted: {
        kind: 'checker',
        steps: [
          { from: 24, to: 21 },
          { from: 24, to: 23 },
        ],
      },
      choice: null,
      evaluated: { chosen: '24/21 24/23', best: '8/5 6/5' },
    },
    {
      item: {
        ...question(2),
        kind: 'cube_offer',
        position: { ...position, dice: [0, 0] },
      },
      submitted: { kind: 'cube_offer', action: 'double' },
      choice: 'Double',
      evaluated: { chosen: 'double', best: 'no double' },
    },
    {
      item: {
        ...question(3),
        kind: 'cube_response',
        position: { ...position, dice: [0, 0] },
      },
      submitted: { kind: 'cube_response', action: 'take' },
      choice: 'Take double',
      evaluated: { chosen: 'take', best: 'drop' },
    },
  ];
  await page.route('**/api/study/mistakes/practice', (r) =>
    r.fulfill({ json: { items: cases.map((c) => c.item) } }),
  );
  let attempts = 0;
  await page.route('**/api/study/mistakes/attempt', async (r) => {
    const entry = cases[attempts++];
    expect(r.request().postDataJSON()).toEqual({
      reference: entry.item.reference,
      answer: entry.submitted,
    });
    await r.fulfill({
      json: {
        original: answer,
        evaluation: {
          verdict: 'error',
          ...entry.evaluated,
          chosen_equity: 0,
          best_equity: 0.125,
          equity_loss: 0.125,
          provenance: { kind: 'recomputed', engine: 'GNUbg', plies: 3 },
        },
      },
    });
  });
  await open(page);
  await page
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await page.getByRole('button', { name: 'Practice', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  for (const [index, entry] of cases.entries()) {
    const check = dialog.getByRole('button', {
      name: entry.choice ? 'Check answer' : 'Check move',
      exact: true,
    });
    await expect(check).toBeDisabled();
    await expect(dialog.locator('.recurring-attempt-result')).toHaveCount(0);
    if (entry.choice) {
      if (entry.item.kind === 'cube_response')
        await expect(
          dialog.locator('[data-cube-pending="true"][data-cube="2"]'),
        ).toBeVisible();
      await dialog
        .getByRole('button', { name: entry.choice, exact: true })
        .click();
    } else {
      const point = dialog.getByRole('button', {
        name: 'Point 24',
        exact: true,
      });
      await point.click();
      await expect(check).toBeDisabled();
      await point.click();
    }
    await check.click();
    await expect(dialog.locator('.recurring-attempt-result')).toContainText(
      '0.125',
    );
    await expect(dialog.locator('.recurring-answer')).toContainText(
      answer.played,
    );
    if (!entry.choice) {
      await dialog
        .getByRole('button', { name: 'Your move', exact: true })
        .click();
      await expect(dialog.locator('.move-arrows > [data-from]')).toHaveCount(2);
      for (const [width, height] of [
        [1800, 1169],
        [2560, 1440],
        [3840, 2160],
      ]) {
        await page.setViewportSize({ width, height });
        await expect(dialog.locator('.recurring-attempt-result')).toBeVisible();
        const box = await dialog.boundingBox();
        expect(box && box.x >= 0 && box.x + box.width <= width).toBe(true);
        await page.screenshot({
          path: test
            .info()
            .outputPath(`recurring-answer-${width}x${height}.png`),
        });
      }
      await page.setViewportSize({ width: 1800, height: 1169 });
    }
    await dialog
      .getByRole('button', {
        name: index === 2 ? 'Finish' : 'Next',
        exact: true,
      })
      .click();
  }
  expect(attempts).toBe(3);
  await expect(dialog).toContainText('Practice complete');
});

test('both cube response groups open a round with correct takes and passes', async ({
  page,
}) => {
  const rounds = [
    {
      label: 'Wrong take',
      kind: 'wrong_take',
      decisions: [
        { action: 'take', played: 'take', loss: 0 },
        { action: 'drop', played: 'take', loss: 0.2 },
      ],
    },
    {
      label: 'Wrong pass',
      kind: 'wrong_pass',
      decisions: [
        { action: 'take', played: 'drop', loss: 0.15 },
        { action: 'drop', played: 'drop', loss: 0 },
      ],
    },
  ] as const;
  await page.route('**/api/study/mistakes?*', (route) =>
    route.fulfill({
      json: {
        ...result,
        groups: new URL(route.request().url()).searchParams.has('player')
          ? rounds.map(({ kind }) => ({ ...group, mistake_kind: kind }))
          : [],
      },
    }),
  );
  const items = [1, 2].map((n) => ({
    ...question(n),
    kind: 'cube_response',
    position: { ...position, dice: [0, 0] },
  }));
  let roundIndex = 0;
  let answerIndex = 0;
  await page.route('**/api/study/mistakes/practice', (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      player: 'Alice',
      game_plan: 'priming',
      mistake_kind: rounds[roundIndex].kind,
    });
    return route.fulfill({ json: { items } });
  });
  await page.route('**/api/study/mistakes/attempt', (route) => {
    const { action, played, loss } = rounds[roundIndex].decisions[answerIndex];
    expect(route.request().postDataJSON()).toEqual({
      reference: items[answerIndex].reference,
      answer: { kind: 'cube_response', action },
    });
    const bestEquity = action === 'take' ? -0.75 : -1;
    return route.fulfill({
      json: {
        original: { ...answer, best: action, played, equity_loss: loss },
        evaluation: {
          verdict: 'best',
          chosen: action,
          best: action,
          chosen_equity: bestEquity,
          best_equity: bestEquity,
          equity_loss: 0,
          provenance: { kind: 'stored', engine: 'GNUbg', plies: 3 },
        },
      },
    });
  });

  await open(page);
  await page.getByRole('combobox', { name: 'Player' }).selectOption('Alice');
  for (const [index, round] of rounds.entries()) {
    roundIndex = index;
    await page
      .getByRole('row', { name: new RegExp(round.label) })
      .getByRole('button', { name: 'Practice', exact: true })
      .click();
    const dialog = page.getByRole('dialog', {
      name: 'Practice recurring mistakes',
    });
    for (const [index, { action }] of round.decisions.entries()) {
      answerIndex = index;
      await expect(dialog.locator('.recurring-answer')).toHaveCount(0);
      const check = dialog.getByRole('button', { name: 'Check answer' });
      await expect(check).toBeDisabled();
      await dialog
        .getByRole('button', {
          name: action === 'take' ? 'Take double' : 'Pass double',
          exact: true,
        })
        .click();
      await check.click();
      await expect(dialog.locator('.recurring-attempt-result')).toContainText(
        '0.000',
      );
      await dialog
        .getByRole('button', { name: index === 0 ? 'Next' : 'Finish' })
        .click();
    }
    await expect(dialog).toContainText('Practice complete');
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
  }
});

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
  const configuration = structuredClone(defaultConfiguration);
  configuration.analysis.error_thresholds.blunder = 0.095;
  await profileBackend(page, { configuration });
  const writes: string[] = [];
  page.on('request', (r) => {
    const p = new URL(r.url()).pathname;
    if (
      r.method() !== 'GET' &&
      p.startsWith('/api/') &&
      // Legal checker paths are computed without persisting anything.
      !/^\/api\/(configuration|profiles|study\/mistakes|play\/checker-turns)(\/|$)/.test(
        p,
      )
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
  await expect(dialog.locator('.recurring-practice-board')).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Point 24', exact: true }),
  ).toBeEnabled();
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
    min_loss: 0.095,
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
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
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
    'Opening',
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

test('a cube question is answered by keyboard and explains score, outcomes and take point', async ({
  page,
}) => {
  await list(page);
  await page.route('**/api/study/mistakes/practice', (route) =>
    route.fulfill({
      json: {
        items: [1, 2].map((n) => ({
          ...question(n),
          kind: 'cube_offer',
          position: { ...position, gnubg_match_id: 'cIngAAAAAAAE', dice: [] },
        })),
      },
    }),
  );
  const attempts: unknown[] = [];
  await page.route('**/api/study/mistakes/attempt', (route) => {
    attempts.push(route.request().postDataJSON().answer);
    return route.fulfill({
      json: {
        original: { ...answer, best: 'double', played: 'no double' },
        evaluation: {
          verdict: 'best',
          chosen: 'double',
          best: 'double',
          chosen_equity: 0.9,
          best_equity: 0.9,
          equity_loss: 0,
          provenance: { kind: 'stored', engine: 'gnubg', plies: 3 },
          cube_evaluation: {
            unit: 'cubeful',
            no_double: 0.7,
            double_take: 0.9,
            double_pass: 1,
            probabilities: [0.74, 0.3, 0.01, 0.26, 0.05, 0],
          },
        },
      },
    });
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
  // The score reads as points still needed and the cube state is spelled out.
  await expect(dialog.locator('[data-cube-context]')).toHaveText(
    'Alice 7-away, Bob 7-away · Cube 1, centered',
  );
  const double = dialog.getByRole('button', { name: 'Double', exact: true });
  await expect(double).toHaveAttribute('aria-keyshortcuts', 'D');
  await double.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Double (D)');
  // Enter does nothing before a choice; D selects and Enter checks.
  await page.keyboard.press('Enter');
  expect(attempts).toEqual([]);
  await expect(dialog).toContainText('1 / 2');
  await page.keyboard.press('d');
  await expect(double).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Enter');
  await expect(dialog.locator('.cube-outcomes')).toContainText(
    'Double / take · Best+0.900',
  );
  expect(attempts).toEqual([{ kind: 'cube_offer', action: 'double' }]);
  await expect(dialog.locator('.cube-outcomes')).toContainText(
    'Normalized cubeful equity · Values for Alice',
  );
  await expect(dialog.locator('.cube-chances')).toContainText('74.0%');
  await expect(dialog.locator('[data-take-point]')).toHaveText(
    /Take point at this score: \d\d\.\d% \(Kazaross-XG2; no gammons, no later cube use\) Bob wins 26\.0% of the games here\./,
  );
  await page.keyboard.press('Enter');
  await expect(dialog).toContainText('2 / 2');
  await expect(dialog.locator('.cube-outcomes')).toHaveCount(0);
});

test('the independent-play choice filters the statistics and practice and is shared with the overview', async ({
  page,
}) => {
  const queries: (string | null)[] = [];
  await page.route('**/api/study/mistakes?*', (r) => {
    const params = new URL(r.request().url()).searchParams;
    if (params.has('player')) queries.push(params.get('independent_only'));
    return r.fulfill({
      json: { ...result, groups: params.has('player') ? [group] : [] },
    });
  });
  let practiceBody: Record<string, unknown> | undefined;
  await page.route('**/api/study/mistakes/practice', (r) => {
    practiceBody = r.request().postDataJSON();
    return r.fulfill({ json: { items: [question(1)] } });
  });
  await open(page);
  const mistakes = page.locator('#study-panel-mistakes');
  await mistakes
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  const toggle = mistakes.getByRole('checkbox', {
    name: 'Only matches played without learning aids',
    exact: true,
  });
  await expect(toggle).not.toBeChecked();
  await expect.poll(() => queries.at(-1)).toBeNull();
  await toggle.check();
  await expect.poll(() => queries.at(-1)).toBe('true');
  await mistakes
    .getByRole('button', { name: 'Practice', exact: true })
    .first()
    .click();
  await expect.poll(() => practiceBody?.independent_only).toBe(true);
  await page.keyboard.press('Escape');

  // The overview shows the same choice, and a reload keeps it.
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  const overviewToggle = page
    .locator('#study-panel-overview')
    .getByRole('checkbox', {
      name: 'Only matches played without learning aids',
      exact: true,
    });
  await expect(overviewToggle).toBeChecked();
  await page.reload();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await expect(overviewToggle).toBeChecked();
});

test('the loss threshold uses the app language decimal separator', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.locale = 'de';
  configuration.analysis.error_thresholds.blunder = 0.093;
  const profiles = await profileBackend(page, { configuration });
  const calls: URL[] = [];
  await page.route('**/api/study/mistakes?*', (r) => {
    calls.push(new URL(r.request().url()));
    return r.fulfill({ json: { ...result, groups: [] } });
  });
  await open(page, 'de');
  const loss = page.getByRole('spinbutton', {
    name: 'Equity-Verlust ab',
    exact: true,
  });
  // Native number inputs would follow the operating system instead.
  await expect(loss).toHaveValue('0,093');
  await expect
    .poll(() => calls.at(-1)?.searchParams.get('min_loss'))
    .toBe('0.093');
  configuration.analysis.error_thresholds.blunder = 0.099;
  profiles.updateRemote(configuration);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(loss).toHaveValue('0,099');
  await loss.fill('0,1');
  await expect(loss).toHaveValue('0,1');
  await expect
    .poll(() => calls.at(-1)?.searchParams.get('min_loss'))
    .toBe('0.1');
  await loss.press('ArrowUp');
  await expect(loss).toHaveValue('0,11');
  await expect(loss).toHaveAttribute('aria-valuenow', '0.11');
  // A dot is accepted too and shown in the app language.
  await loss.fill('0.2');
  await expect(loss).toHaveValue('0,2');
  await expect
    .poll(() => calls.at(-1)?.searchParams.get('min_loss'))
    .toBe('0.2');
  configuration.analysis.error_thresholds.blunder = 0.088;
  profiles.updateRemote(configuration);
  const refreshed = page.waitForResponse('**/api/configuration');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await refreshed;
  await expect(loss).toHaveValue('0,2');
});

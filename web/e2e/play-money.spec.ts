import { test, expect } from './test';
import { playMenuItem } from './play-board-actions';
test.use({ actionTimeout: 10000 });
import { profileBackend } from './profile-backend';
import { historyFixture } from './play-history-review-helpers';
test('Money game selection applies to the new session and leaves the profile defaults intact', async ({
  page,
}) => {
  const profile = await profileBackend(page);
  const { saved } = await historyFixture(page);
  let payload: unknown;
  const money = {
    ...saved,
    match_length: 0,
  };
  await page.route('**/api/play/sessions', (r) => {
    payload = r.request().postDataJSON();
    return r.fulfill({ status: 201, json: money });
  });
  await (await playMenuItem(page, 'New match')).click();
  await page
    .getByRole('combobox', { name: 'Match length', exact: true })
    .selectOption({ label: 'Money game' });
  await page
    .locator('.play-setup')
    .getByRole('button', { name: 'Start match', exact: true })
    .click();
  await expect.poll(() => payload).toMatchObject({ match_length: 0 });
  await expect(page.locator('.play-match-heading')).toContainText('Money game');
  await expect(page.locator('.play-match-heading')).not.toContainText(
    '0 points',
  );
  expect(profile.calls.filter(({ method }) => method !== 'GET')).toEqual([]);
  await (await playMenuItem(page, 'New match')).click();
  await expect(
    page.getByRole('combobox', { name: 'Match length', exact: true }),
  ).toHaveValue('7');
});

test('starts a match while preserving conflicting profile changes for recovery', async ({
  page,
}) => {
  const profile = await profileBackend(page);
  const local = structuredClone(profile.state.current_configuration);
  local.general.player_name = 'Unsaved local name';
  local.play.match_length = 5;
  const pending = {
    base_revision: profile.state.revision,
    configuration: local,
  };
  profile.updateRemote({
    ...profile.state.current_configuration,
    general: { ...local.general, player_name: 'Confirmed name' },
  });
  await page.addInitScript((draft) => {
    localStorage.setItem(
      'bgtutor.configuration-pending.v1',
      JSON.stringify(draft),
    );
  }, pending);
  const { saved } = await historyFixture(page);
  let payload: unknown;
  await page.route('**/api/play/sessions', (route) => {
    payload = route.request().postDataJSON();
    return route.fulfill({
      status: 201,
      json: {
        ...saved,
        human_player: 1,
        match_length: 3,
        play_mode: 'training',
      },
    });
  });

  await (await playMenuItem(page, 'New match')).click();
  const setup = page.locator('.play-setup');
  await setup.getByRole('radio', { name: 'Black', exact: true }).check();
  await setup
    .getByRole('combobox', { name: 'Match length', exact: true })
    .selectOption('3');
  await setup.getByRole('radio', { name: 'Training', exact: true }).check();
  await setup.getByLabel('Open training analysis automatically').uncheck();
  await setup.getByRole('button', { name: 'Start match', exact: true }).click();

  await expect(page.locator('.play-match-heading')).toContainText('3 points');
  expect(payload).toMatchObject({
    human_player: 1,
    match_length: 3,
    play_mode: 'training',
    training_auto_analysis: false,
  });
  expect(profile.calls.filter(({ method }) => method !== 'GET')).toEqual([]);
  expect(
    await page.evaluate(() =>
      JSON.parse(localStorage.getItem('bgtutor.configuration-pending.v1')!),
    ),
  ).toEqual(pending);
  await (await playMenuItem(page, 'New match')).click();
  await expect(
    setup.getByRole('combobox', { name: 'Match length', exact: true }),
  ).toHaveValue('7');
});

test('money games continue with the score and keep each saved game report', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  let reviews = 0;
  Object.assign(saved, {
    match_length: 0,
    phase: 'game_over',
    scores: [0, 3],
    game_no: 1,
    winner: 1,
    review: null,
  });
  const reports: Record<string, unknown>[] = [];
  await page.route(`**/api/play/sessions/${saved.id}/review`, async (route) => {
    reviews++;
    const { detail } = await import('./study-report-fixture');
    const report = detail().review;
    report.games[0].game_no = saved.game_no;
    Object.assign(saved, {
      version: saved.version + 1,
      review: report,
      study_report_id: `66bb9950-416c-4bde-9241-bc2fd47dc05${saved.game_no}`,
      study_report_version: saved.version + 1,
      review_game_no: saved.game_no,
    });
    reports.push(structuredClone(saved));
    await route.fulfill({ json: saved });
  });
  await page.route(
    `**/api/play/sessions/${saved.id}/actions`,
    async (route) => {
      const { action } = route.request().postDataJSON();
      if (action === 'opening_roll')
        Object.assign(saved, {
          version: saved.version + 1,
          phase: 'opening_result',
          opening: { white_die: 3, black_die: 1, winner: 0, rolls: [[3, 1]] },
        });
      else if (action === 'start_opening_move')
        Object.assign(saved, { version: saved.version + 1, phase: 'roll' });
      else {
        expect(action).toBe('next_game');
        Object.assign(saved, {
          version: saved.version + 1,
          phase: 'opening_roll',
          game_no: saved.game_no + 1,
          review: null,
          study_report_id: null,
          study_report_version: null,
          winner: null,
        });
      }
      await route.fulfill({ json: saved });
    },
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const next = page.getByRole('button', { name: 'Next game', exact: true });
  await expect(next).toBeEnabled();
  expect(reviews).toBe(1);
  const first = structuredClone(reports[0]);
  await page
    .getByRole('alertdialog', { name: 'Game finished', exact: true })
    .getByRole('button', { name: 'Learning review', exact: true })
    .click();
  const reportDialog = page.getByRole('dialog', {
    name: 'Learning review',
    exact: true,
  });
  await expect(reportDialog).toBeVisible();
  await page.keyboard.press('Escape');
  await next.click();
  await expect(page.locator('.play-match-heading')).toContainText('0 : 3');
  await expect(page.locator('.play-match-heading')).toContainText('Game 2');
  Object.assign(saved, {
    version: saved.version + 1,
    phase: 'game_over',
    scores: [0, 6],
    winner: 1,
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(next).toBeEnabled();
  expect(reviews).toBe(2);
  expect(reports[0]).toEqual(first);
  expect(reports[1].scores).toEqual([0, 6]);
  expect(reports[1].study_report_id).not.toBe(reports[0].study_report_id);
});

for (const [width, height, locale] of [
  [1800, 1169, 'en'],
  [390, 1000, 'en'],
  [390, 1000, 'de'],
] as const) {
  test(`money sessions can pause beside Next game and resume after reload at ${width}px (${locale})`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height });
    const { saved } = await historyFixture(page);
    const german = locale === 'de';
    const text = {
      play: german ? 'Spielen' : 'Playing',
      result: german ? 'Spiel beendet' : 'Game finished',
      next: german ? 'Nächstes Spiel' : 'Next game',
      end: german ? 'Match pausieren' : 'Pause match',
      paused: german ? 'Match pausiert' : 'Match paused',
      resume: german ? 'Match fortsetzen' : 'Resume match',
      review: german ? 'Lernauswertung' : 'Learning review',
      reviewButton: german ? 'Spielauswertung' : 'Learning review',
    };
    if (german)
      await page.evaluate(async () => {
        const state = await (await fetch('/api/configuration')).json();
        const response = await fetch('/api/configuration', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            expected_revision: state.revision,
            configuration: {
              ...state.current_configuration,
              general: { ...state.current_configuration.general, locale: 'de' },
            },
          }),
        });
        if (!response.ok) throw new Error(await response.text());
      });
    const { detail } = await import('./study-report-fixture');
    Object.assign(saved, {
      match_length: 0,
      phase: 'game_over',
      scores: [3, 6],
      game_no: 3,
      winner: 1,
      review: detail().review,
    });
    const report = structuredClone(saved.review);
    const requests: unknown[] = [];
    let fail = true;
    await page.route(
      `**/api/play/sessions/${saved.id}/actions`,
      async (route) => {
        const body = route.request().postDataJSON();
        requests.push(body);
        if (body.action === 'resume') {
          expect(body).toEqual({ version: 8, action: 'resume' });
          Object.assign(saved, { paused: false, version: 9 });
          return route.fulfill({ json: saved });
        }
        expect(body).toEqual({ version: 7, action: 'pause' });
        if (fail) {
          fail = false;
          return route.fulfill({
            status: 503,
            json: { error: 'Saved session is unchanged' },
          });
        }
        Object.assign(saved, { paused: true, version: 8 });
        return route.fulfill({ json: saved });
      },
    );
    await page.reload();
    await page.getByRole('tab', { name: text.play, exact: true }).click();
    const result = page.getByRole('alertdialog', {
      name: text.result,
      exact: true,
    });
    const next = result.getByRole('button', { name: text.next, exact: true });
    const end = result.getByRole('button', {
      name: text.end,
      exact: true,
    });
    await expect(end).toBeEnabled();
    const nextBox = await next.boundingBox(),
      endBox = await end.boundingBox();
    expect(nextBox).toBeTruthy();
    expect(endBox).toBeTruthy();
    expect(endBox!.x).toBeGreaterThan(nextBox!.x);
    expect(Math.abs(endBox!.y - nextBox!.y)).toBeLessThan(2);
    expect(Math.abs(endBox!.height - nextBox!.height)).toBeLessThan(2);
    await end.click();
    await expect(result).toContainText('Saved session is unchanged');
    await expect(next).toBeEnabled();
    await end.click();
    await expect(page.getByText(text.paused, { exact: true })).toBeVisible();
    await expect(next).toHaveCount(0);
    await expect(page.locator('.play-match-heading')).toContainText('3 : 6');
    expect(saved.review).toEqual(report);

    await page.reload();
    await page.getByRole('tab', { name: text.play, exact: true }).click();
    await expect(page.getByText(text.paused, { exact: true })).toBeVisible();
    expect(requests).toHaveLength(2);
    await page.getByRole('button', { name: text.resume, exact: true }).click();
    await expect(result).toBeVisible();
    await expect(next).toBeEnabled();
    const directReview = result.getByRole('button', {
      name: text.reviewButton,
      exact: true,
    });
    if (german) {
      await page.clock.install();
      await directReview.focus();
      await expect(directReview.getByRole('tooltip')).toHaveCount(0);
      await directReview.blur();
      await directReview.hover();
      await page.clock.runFor(400);
      await expect(directReview.getByRole('tooltip')).toHaveCount(0);
    }
    await directReview.click();
    const reportDialog = page.getByRole('dialog', {
      name: text.review,
      exact: true,
    });
    await expect(reportDialog).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(reportDialog).toHaveCount(0);
    await expect(directReview).toBeFocused();
    await expect(page.locator('.play-match-heading')).toContainText('3 : 6');
    expect(requests).toHaveLength(3);
  });
}

test('a money session can be ended after a finished game', async ({ page }) => {
  const { saved } = await historyFixture(page);
  const { detail } = await import('./study-report-fixture');
  Object.assign(saved, {
    match_length: 0,
    phase: 'game_over',
    scores: [3, 6],
    game_no: 3,
    winner: 1,
    review: detail().review,
  });
  const requests: unknown[] = [];
  await page.route(`**/api/play/sessions/${saved.id}/actions`, (route) => {
    requests.push(route.request().postDataJSON());
    Object.assign(saved, {
      version: saved.version + 1,
      phase: 'match_over',
      money_session_ended: true,
    });
    return route.fulfill({ json: saved });
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const result = page.getByRole('alertdialog', {
    name: 'Game finished',
    exact: true,
  });
  // Resigning only applies to a game in progress.
  await expect(
    page.getByRole('button', { name: 'Resign game', exact: true }),
  ).toHaveCount(0);
  await result
    .getByRole('button', { name: 'End session', exact: true })
    .click();
  const ended = page.getByRole('alertdialog', {
    name: 'Session ended',
    exact: true,
  });
  await expect(ended).toBeVisible();
  await expect(
    ended.getByRole('button', { name: 'New match', exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'End session', exact: true }),
  ).toHaveCount(0);
  await expect(page.locator('.play-match-heading')).toContainText('3 : 6');
  expect(requests).toEqual([{ version: 7, action: 'end_session' }]);
});

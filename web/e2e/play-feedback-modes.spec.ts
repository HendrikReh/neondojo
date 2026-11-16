import { applyAndCloseSettings } from './settings-actions';
import { test, expect } from './test';
const id = '7a8f98a9-8b48-4d76-95fd-c8c8d4ced0ab';
const decision = {
  id: '07aab353-8dc9-40c8-91a9-3377483240b5',
  action: 'move',
  move_index: 0,
  original_action: 'move',
  original_move_index: 0,
  warning: true,
  revealed: false,
  assisted: false,
  retry_count: 0,
  first_loss: null,
  loss: null,
  first_severity: null,
  severity: null,
  alternatives: [],
  attempts: [],
  committed: false,
};
function saved(mode = 'coach', pending: unknown = null) {
  return {
    id,
    version: 1,
    human_player: 0,
    human_name: 'Learner',
    match_length: 7,
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: 'GNU Backgammon native', plies: 0 },
    play_mode: mode,
    analysis_plies: 2,
    tutor_threshold: 0.04,
    training_auto_analysis: true,
    pending_decision: pending,
    decisions: [],
    position: {
      id,
      match_uuid: id,
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: 0,
      dice: [3, 1],
      board: [
        0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0,
        0, 2, 0,
      ],
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: '',
    },
    phase: 'move',
    can_double: false,
    legal_moves: [
      {
        notation: '8/5 6/5',
        steps: [
          { from: 8, to: 5 },
          { from: 6, to: 5 },
        ],
      },
    ],
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
  };
}
async function resume(
  page: import('@playwright/test').Page,
  state: ReturnType<typeof saved>,
) {
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.play-session', id);
    localStorage.setItem('bgtutor.play-sounds', 'off');
  }, id);
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: state }),
  );
  await page.goto('/');
  await page.locator('#workspace-tab-play').click();
  if (state.phase === 'match_over')
    await page
      .getByRole('button', { name: 'Back to board', exact: true })
      .click();
  else if (
    state.play_mode !== 'coach' &&
    state.pending_decision &&
    (state.pending_decision as { action?: string }).action
  )
    await page.locator('.play-dialog-preferences summary').click();
}
test('Coach is the default mode and all four profiles are selectable', async ({
  page,
}) => {
  await page.route('**/api/settings', (route) =>
    route.fulfill({
      json: {
        version: 0,
        player_name: 'Learner',
        analysis_engine: 'gnubg',
        analysis_plies: 2,
        play_engine: 'gnubg',
        play_plies: 0,
        gnubg_path: 'gnubg',
        default_play_mode: 'coach',
        tutor_threshold: 0.04,
        training_auto_analysis: true,
      },
    }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(
    page.locator('.play-setup').getByLabel('Play mode', { exact: true }),
  ).toHaveValue('coach');
  await expect(
    page
      .locator('.play-setup')
      .getByLabel('Play mode', { exact: true })
      .locator('option'),
  ).toHaveCount(4);
});
test('Coach shows deltas while hiding absolute equity and probabilities', async ({
  page,
}) => {
  await resume(
    page,
    saved('coach', {
      ...decision,
      revealed: true,
      warning: false,
      loss: 0.08,
      first_loss: 0.08,
      severity: 'bad',
      first_severity: 'bad',
      alternatives: [
        {
          action: 'move',
          move_index: 0,
          notation: '8/5 6/5',
          loss: 0,
          equity: 0.777,
          probabilities: [0.991, 0.45, 0.1, 0.009, 0.001, 0],
        },
      ],
    }),
  );
  const panel = page.getByTestId('play-feedback');
  await expect(panel).toBeVisible();
  await expect(panel).toContainText('0.080');
  await expect(panel).not.toContainText('0.777');
  await expect(panel).not.toContainText('99.1');
  await expect(page.locator('[data-play-point="8"]')).toHaveAttribute(
    'data-enabled',
    'false',
  );
});
for (const [grade, token, loss, width, height] of [
  ['doubtful', '--ui-warning-text', 0.01, 1800, 1169],
  ['bad', '--ui-bad-text', 0.08, 2560, 1440],
  ['blunder', '--ui-danger-text', 0.16, 3840, 2160],
  ['very_bad', '--ui-danger-text', 0.3, 3840, 2160],
] as const) {
  test(`Coach highlights a ${grade} decision using the review color`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height });
    await page.emulateMedia({ colorScheme: width === 1800 ? 'light' : 'dark' });
    await resume(
      page,
      saved('coach', { ...decision, revealed: true, loss, severity: grade }),
    );
    const panel = page.getByTestId('play-feedback');
    const choice = panel.getByText('8/5 6/5', { exact: true });
    const color = await choice.evaluate((element, property) => {
      const sample = document.createElement('span');
      sample.style.color = `var(${property})`;
      element.append(sample);
      const color = getComputedStyle(sample).color;
      sample.remove();
      return color;
    }, token);
    await expect(choice).toHaveCSS('color', color);
    await expect(panel.locator('.play-feedback-result')).toHaveCSS(
      'color',
      color,
    );
  });
}

test('Tutor restores a neutral warning and reveals only on request', async ({
  page,
}) => {
  const state = saved('tutor', decision);
  await resume(page, state);
  const panel = page.getByTestId('play-feedback');
  await expect(panel).toContainText('reconsider');
  await expect(panel.getByRole('heading', { level: 3 })).toHaveText(
    'Decision feedback: 8/5 6/5',
  );
  await expect(panel).not.toContainText('Alternatives');
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(panel).toContainText('reconsider');
  await page.route(`**/api/play/sessions/${id}/feedback`, async (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      version: 1,
      operation: 'reveal',
      decision_id: decision.id,
    });
    await route.fulfill({
      json: {
        ...state,
        version: 2,
        pending_decision: {
          ...decision,
          revealed: true,
          loss: 0.09,
          first_loss: 0.09,
          severity: 'bad',
          first_severity: 'bad',
          alternatives: [
            {
              action: 'move',
              move_index: 0,
              notation: '8/5 6/5',
              loss: 0,
              equity: 0.3,
              probabilities: [0.6, 0.1, 0, 0.4, 0.1, 0],
            },
          ],
        },
      },
    });
  });
  await panel
    .getByRole('button', { name: 'Show solution', exact: true })
    .click();
  await expect(panel).toContainText('8/5 6/5');
  await expect(panel).toContainText('First attempt');
});
test('Competition hides pip counts and live feedback', async ({ page }) => {
  const state = saved('competition');
  state.engine.version = 'GNU Backgammon 1.08.003 20261108';
  await resume(page, state);
  const engine = page.locator('.play-match-engine .engine-label-details');
  await expect(engine).toHaveText('GNU Backgammon 1.08.003');
  await engine.hover();
  await expect(page.getByRole('tooltip')).toHaveText(
    'GNU Backgammon 1.08.003 20261108 · 0-ply · Bear off to the right',
  );
  await expect(page.getByTestId('play-feedback')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Hint', exact: true }),
  ).toHaveCount(0);
  await expect(
    page.locator('.play-board .player-bar').first(),
  ).not.toContainText('167');
});

test('Hint is absent when the cube is unavailable before rolling', async ({
  page,
}) => {
  const state = saved('coach');
  state.phase = 'roll';
  state.position.dice = [];
  state.legal_moves = [];
  await resume(page, state);
  await expect(
    page.getByRole('button', { name: 'Hint', exact: true }),
  ).toHaveCount(0);
});
test('completed review identifies the original and corrected choices with help status', async ({
  page,
}) => {
  const state = {
    ...saved('tutor'),
    phase: 'match_over',
    decisions: [
      {
        ...decision,
        committed: true,
        revealed: true,
        retry_count: 1,
        game_no: 1,
        history_index: 0,
        original_notation: '24/21 6/5',
        notation: '8/5 6/5',
        first_loss: 0.09,
        loss: 0,
        assisted: true,
        attempts: [
          {
            action: 'move',
            move_index: 1,
            notation: '24/21 6/5',
            loss: 0.09,
            severity: 'bad',
            assisted: false,
          },
          {
            action: 'move',
            move_index: 0,
            notation: '8/5 6/5',
            loss: 0,
            severity: 'none',
            assisted: true,
          },
        ],
      },
    ],
    review: {
      analysis_plies: 2,
      assisted: true,
      first_attempt_loss: 0.09,
      final_choice_loss: 0,
      first_attempt_count: 1,
      retry_count: 1,
      games: [],
    },
  };
  await resume(page, state);
  await page
    .getByRole('button', { name: 'Learning review', exact: true })
    .click();
  const review = page.getByRole('region', {
    name: 'Learning review',
    exact: true,
  });
  await expect(review).toContainText('24/21 6/5');
  await expect(review).toContainText('8/5 6/5');
  await review.locator('summary').click();
  await expect(review.locator('ol')).toContainText('Independent');
  await expect(review.locator('ol')).toContainText('With help');
});

test('Analysis thresholds and the default play mode apply to new matches', async ({
  page,
}) => {
  const settings = {
    version: 0,
    player_name: 'Learner',
    analysis_engine: 'gnubg',
    analysis_plies: 2,
    play_engine: 'gnubg',
    play_plies: 0,
    gnubg_path: 'gnubg',
    default_play_mode: 'coach',
    tutor_threshold: 0.04,
    training_auto_analysis: true,
  };
  await page.route('**/api/settings', async (route) => {
    await route.fulfill({ json: settings });
  });
  await page.goto('/');
  await page
    .getByRole('button', { name: 'Settings', exact: true })
    .first()
    .click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await dialog
    .getByLabel('Default play mode', { exact: true })
    .selectOption('tutor');
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  const threshold = dialog.getByLabel('Error', { exact: true });
  await expect(threshold).toHaveValue('0.020');
  await threshold.fill('0.005');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('strictly increasing');
  await threshold.fill('0,025');
  await applyAndCloseSettings(dialog);
  const configuration = await page.evaluate(async () => {
    const response = await fetch('/api/configuration');
    return response.json();
  });
  expect(configuration.current_configuration.analysis.error_thresholds.error).toBe(0.025);
  expect(configuration.current_configuration.analysis.plies).toBe(2);
  expect(configuration.current_configuration.play.plies).toBe(0);
  expect(configuration.active_profile.configuration.analysis.error_thresholds.error).toBe(
    0.02,
  );
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(
    page.locator('.play-setup').getByLabel('Play mode', { exact: true }),
  ).toHaveValue('tutor');
  await expect(
    page.getByText('Tutor warning threshold · 0.025', { exact: true }),
  ).toBeVisible();
});
test('Training displays complete equities and probabilities', async ({
  page,
}) => {
  await resume(
    page,
    saved('training', {
      ...decision,
      revealed: true,
      warning: false,
      loss: 0.08,
      first_loss: 0.08,
      severity: 'bad',
      first_severity: 'bad',
      alternatives: [
        {
          action: 'move',
          move_index: 0,
          notation: '8/5 6/5',
          loss: 0,
          equity: 0.777,
          probabilities: [0.991, 0.45, 0.1, 0.009, 0.001, 0],
        },
      ],
    }),
  );
  const panel = page.getByTestId('play-feedback');
  await expect(panel).toContainText('0.777');
  await expect(panel).toContainText('99.1%');
  await page.screenshot({
    path: '/tmp/bgtutor-training-feedback.png',
    fullPage: true,
  });
});
test('the dice resume a saved Coach move with its decision identity', async ({
  page,
}) => {
  const state = saved('coach', {
    ...decision,
    revealed: true,
    warning: false,
    loss: 0.08,
    first_loss: 0.08,
    severity: 'bad',
    alternatives: [],
  });
  await resume(page, state);
  let submitted = false;
  await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      version: 1,
      action: 'move',
      move: 0,
      decision_id: decision.id,
    });
    submitted = true;
    await route.fulfill({
      json: {
        ...state,
        version: 2,
        pending_decision: null,
        phase: 'roll',
        legal_moves: [],
        history: [{ player: 0, dice: [3, 1], notation: '8/5 6/5' }],
      },
    });
  });
  expect(submitted).toBe(false);
  await expect(page.getByRole('button', { name: 'Play move' })).toHaveCount(0);
  await page.locator('[data-dice-action="confirm"]').click();
  await expect(page.getByTestId('play-feedback')).toHaveCount(0);
  expect(submitted).toBe(true);
});

test('requesting a hint preserves unconfirmed checker steps', async ({
  page,
}) => {
  const state = saved('training');
  await resume(page, state);
  await page.locator('[data-play-point="8"]').click();
  await expect(page.locator('[data-play-point="6"]')).toHaveAttribute(
    'data-enabled',
    'true',
  );
  await page.route(`**/api/play/sessions/${id}/feedback`, (route) =>
    route.fulfill({
      json: {
        ...state,
        version: 2,
        pending_decision: {
          ...decision,
          action: null,
          move_index: null,
          original_action: null,
          original_move_index: null,
          assisted: true,
          revealed: true,
          warning: false,
          alternatives: [],
        },
      },
    }),
  );
  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  const hintView = page.getByRole('dialog', { name: 'Hint', exact: true });
  await expect(hintView.getByTestId('play-feedback')).toBeVisible();
  await hintView.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('[data-play-point="6"]')).toHaveAttribute(
    'data-enabled',
    'true',
  );
});

test('startup restores a live Competition selection into Playing without an analysis error', async ({
  page,
}) => {
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1, positionId: id }),
      ),
    id,
  );
  let restrictedReads = 0;
  await page.route(`**/api/matches/${id}/metadata`, (route) =>
    route.fulfill({
      json: {
        play_mode: 'competition',
        play_phase: 'move',
        play_session_id: id,
      },
    }),
  );
  await page.route(`**/api/matches/${id}`, (route) => {
    restrictedReads++;
    return route.fulfill({
      status: 422,
      json: {
        error: 'Complete the competition match before requesting analysis',
      },
    });
  });
  await page.route(`**/api/positions/${id}`, (route) => {
    restrictedReads++;
    return route.fulfill({
      status: 422,
      json: {
        error: 'Complete the competition match before requesting analysis',
      },
    });
  });
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: saved('competition') }),
  );
  await page.goto('/');
  await expect(
    page.getByRole('tab', { name: 'Playing', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('.play-board')).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  expect(restrictedReads).toBe(0);
});
test('a running match exposes its active mode and persists mode changes on reload', async ({
  page,
}) => {
  const state = saved('competition');
  await resume(page, state);
  const heading = page.locator('.play-match-heading');
  await expect(heading).toContainText('Game 1');
  await expect(heading.getByRole('status')).toHaveText('Your turn');
  await expect(
    page.getByLabel('Tutor warning threshold', { exact: true }),
  ).toHaveCount(0);
  await page.route(`**/api/play/sessions/${id}/mode`, async (route) => {
    expect(route.request().postDataJSON()).toEqual({
      version: 1,
      play_mode: 'coach',
    });
    Object.assign(state, {
      version: 2,
      play_mode: 'coach',
      learning_used: true,
    });
    await route.fulfill({ json: state });
  });
  const mode = heading.getByRole('combobox', {
    name: 'Change play mode',
    exact: true,
  });
  await expect(mode).toHaveValue('competition');
  await mode.selectOption('coach');
  await expect(mode).toHaveValue('coach');
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(
    page.getByLabel('Change play mode', { exact: true }),
  ).toHaveValue('coach');
  await expect(
    page.getByLabel('Tutor warning threshold', { exact: true }),
  ).toHaveCount(0);
});
test('switching pending feedback to Competition keeps a neutral explicit confirmation', async ({
  page,
}) => {
  const state = saved('coach', {
    ...decision,
    revealed: true,
    warning: false,
    loss: 0.08,
    first_loss: 0.08,
    severity: 'bad',
    alternatives: [],
  });
  await resume(page, state);
  await page.route(`**/api/play/sessions/${id}/mode`, (route) =>
    route.fulfill({
      json: {
        ...state,
        version: 2,
        play_mode: 'competition',
        learning_used: true,
        pending_decision: { ...decision, warning: false },
      },
    }),
  );
  await page
    .getByRole('combobox', { name: 'Change play mode', exact: true })
    .selectOption('competition');
  const panel = page.getByTestId('play-feedback');
  await expect(panel).toContainText('Unconfirmed decision');
  await expect(panel).not.toContainText('Equity');
  await expect(
    panel.getByRole('button', { name: 'Continue', exact: true }),
  ).toBeVisible();
});

test('Home cancels a delayed startup Competition redirect', async ({
  page,
}) => {
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      ),
    id,
  );
  let release = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  await page.route(`**/api/matches/${id}/metadata`, async (route) => {
    await gate;
    await route
      .fulfill({
        json: {
          play_mode: 'competition',
          play_phase: 'move',
          play_session_id: id,
        },
      })
      .catch(() => {});
  });
  await page.goto('/');
  await page.getByRole('link', { name: 'BG Tutor home', exact: true }).click();
  release();
  await expect(page.locator('.analysis-welcome')).toBeVisible();
  await expect(
    page.getByRole('tab', { name: 'Analysis', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('alert')).toHaveCount(0);
});
test('mode switching preserves partial checker steps', async ({ page }) => {
  const state = saved('training');
  await resume(page, state);
  await page.locator('[data-play-point="8"]').click();
  await page.route(`**/api/play/sessions/${id}/mode`, (route) =>
    route.fulfill({
      json: { ...state, version: 2, play_mode: 'coach', learning_used: true },
    }),
  );
  await page
    .getByLabel('Change play mode', { exact: true })
    .selectOption('coach');
  await expect(
    page.getByLabel('Change play mode', { exact: true }),
  ).toHaveValue('coach');
  await expect(page.locator('[data-play-point="6"]')).toHaveAttribute(
    'data-enabled',
    'true',
  );
  await page.screenshot({
    path: '/tmp/bgtutor-active-mode.png',
    fullPage: true,
  });
});

for (const choice of ['open', 'upload'] as const) {
  test(`welcome ${choice} cancels a delayed Competition restoration`, async ({
    page,
  }) => {
    await page.addInitScript(
      (id) =>
        localStorage.setItem(
          'bgtutor.workspace',
          JSON.stringify({ matchId: id, gameNo: 1 }),
        ),
      id,
    );
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    await page.route(`**/api/matches/${id}/metadata`, async (route) => {
      await gate;
      await route
        .fulfill({
          json: {
            play_mode: 'competition',
            play_phase: 'move',
            play_session_id: id,
          },
        })
        .catch(() => {});
    });
    await page.route(/\/api\/imports(?:\?|$)/, (route) =>
      route.fulfill({
        status: 422,
        json: { error: 'Fixture import rejected' },
      }),
    );
    await page.goto('/');
    if (choice === 'open') {
      await page
        .getByRole('button', { name: 'Open match', exact: true })
        .click();
    } else {
      const data = await page.evaluateHandle(() => {
        const transfer = new DataTransfer();
        transfer.items.add(new File(['fixture'], 'fixture.sgf'));
        return transfer;
      });
      await page
        .locator('.match-drop-zone')
        .dispatchEvent('drop', { dataTransfer: data });
      await expect(page.getByRole('alert')).toContainText(
        'Fixture import rejected',
      );
    }
    release();
    await expect(
      page.getByRole('tab', {
        name: choice === 'open' ? 'Match database' : 'Analysis',
        exact: true,
      }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(
      page.getByRole('tab', { name: 'Playing', exact: true }),
    ).toHaveAttribute('aria-selected', 'false');
  });
}

test('entering Competition synchronously discards cached linked analysis', async ({
  page,
}) => {
  const state = { ...saved('coach'), library_match_id: id };
  await page.addInitScript((id) => {
    localStorage.setItem(
      'bgtutor.workspace',
      JSON.stringify({ matchId: id, gameNo: 1 }),
    );
    localStorage.setItem('bgtutor.play-session', id);
  }, id);
  await page.route(`**/api/matches/${id}/metadata`, (route) => {
    return route.fulfill({
      json: { play_mode: 'coach', play_phase: 'move', play_session_id: id },
    });
  });
  await page.route(`**/api/matches/${id}`, (route) =>
    route.fulfill({
      json: {
        id,
        source_available: true,
        filename: 'mode-cache.sgf',
        format: 'sgf',
        white: 'Learner',
        black: 'Opponent',
        match_length: 7,
        position_count: 1,
        created_at: '2026-11-10T00:00:00Z',
      },
    }),
  );
  await page.route(`**/api/matches/${id}/games`, (route) =>
    route.fulfill({
      json: [
        {
          game_no: 1,
          position_count: 1,
          white_score: 0,
          black_score: 0,
          crawford: false,
        },
      ],
    }),
  );
  await page.route(`**/api/matches/${id}/dice?*`, (route) =>
    route.fulfill({ json: { total: 0, rolls: [] } }),
  );
  await page.route('**/api/positions?*', (route) =>
    route.fulfill({ json: { items: [state.position], next_cursor: null } }),
  );
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: state }),
  );
  await page.route(`**/api/play/sessions/${id}/mode`, (route) =>
    route.fulfill({
      json: {
        ...state,
        version: 2,
        play_mode: 'competition',
        learning_used: true,
      },
    }),
  );
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page
    .getByLabel('Change play mode', { exact: true })
    .selectOption('competition');
  await expect(
    page.getByLabel('Change play mode', { exact: true }),
  ).toHaveValue('competition');
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(page.locator('.analysis-workspace .board')).toHaveCount(0);
});

test('running Tutor threshold saves on blur without a button or duplicate writes', async ({
  page,
}) => {
  const state = { ...saved('tutor'), tutor_threshold: 0.04 };
  await resume(page, state);
  const threshold = page.getByLabel('Tutor warning threshold', { exact: true });
  await expect(
    page.getByRole('button', { name: 'Apply threshold', exact: true }),
  ).toHaveCount(0);
  await expect(threshold).toHaveValue('0.04');
  await page.locator('[data-play-point="8"]').click();
  let writes = 0;
  await page.route(`**/api/play/sessions/${id}/mode`, (route) => {
    writes++;
    expect(route.request().postDataJSON()).toEqual({
      version: 1,
      play_mode: 'tutor',
      tutor_threshold: 0.075,
    });
    Object.assign(state, { version: 2, tutor_threshold: 0.075 });
    return route.fulfill({ json: state });
  });
  for (const invalid of ['0', '1.1', '']) {
    await threshold.fill(invalid);
    await threshold.press('Tab');
    await expect(threshold).toHaveValue('0.04');
  }
  expect(writes).toBe(0);
  await threshold.fill('0.075');
  expect(writes).toBe(0);
  await threshold.press('Tab');
  await expect.poll(() => writes).toBe(1);
  await expect(threshold).toBeEnabled();
  await expect(page.locator('[data-play-point="6"]')).toHaveAttribute(
    'data-enabled',
    'true',
  );
  await threshold.focus();
  await threshold.press('Tab');
  expect(writes).toBe(1);
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(threshold).toHaveValue('0.075');
  await page.screenshot({
    path: '/tmp/bgtutor-blur-threshold.png',
    fullPage: true,
  });
});

test('failed automatic threshold saves retain the draft for retry on blur', async ({
  page,
}) => {
  const state = { ...saved('tutor'), tutor_threshold: 0.04 };
  await resume(page, state);
  const threshold = page.getByLabel('Tutor warning threshold', { exact: true });
  let release = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  let writes = 0;
  await page.route(`**/api/play/sessions/${id}/mode`, async (route) => {
    writes++;
    if (writes === 1) {
      await gate;
      await route.fulfill({
        status: 409,
        json: { error: 'Session changed; reload before changing playing mode' },
      });
    } else {
      Object.assign(state, { version: 2, tutor_threshold: 0.09 });
      await route.fulfill({ json: state });
    }
  });
  const status = page.locator('.play-threshold-control [role="status"]');
  await threshold.fill('0.09');
  await threshold.press('Tab');
  await expect(threshold).toBeDisabled();
  await expect(status).toHaveText('Saving…');
  await expect(
    page.getByLabel('Change play mode', { exact: true }),
  ).toBeDisabled();
  release();
  await expect(page.getByRole('alert')).toContainText('Session changed');
  await expect(status).toHaveText('');
  await expect(threshold).toBeEnabled();
  await expect(threshold).toHaveValue('0.09');
  await threshold.focus();
  await threshold.press('Enter');
  await expect.poll(() => writes).toBe(2);
  await expect(threshold).toBeEnabled();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(status).toHaveText('Saved');
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(threshold).toHaveValue('0.09');
});

test('completed matches keep the Tutor threshold read-only', async ({
  page,
}) => {
  await resume(page, {
    ...saved('tutor'),
    phase: 'match_over',
    tutor_threshold: 0.04,
  });
  await expect(
    page.getByLabel('Tutor warning threshold', { exact: true }),
  ).toBeDisabled();
});

for (const locale of ['en', 'de'] as const) {
  test(`threshold save status reports progress and temporary success in ${locale}`, async ({
    page,
  }) => {
    await page.addInitScript(
      (locale) => localStorage.setItem('bgtutor.locale', locale),
      locale,
    );
    const state = { ...saved('tutor'), tutor_threshold: 0.04 };
    await resume(page, state);
    const threshold = page.getByLabel(
      locale === 'de' ? 'Tutor-Warnschwelle' : 'Tutor warning threshold',
      { exact: true },
    );
    const status = page.locator('.play-threshold-control [role="status"]');
    await expect(status).toHaveText('');
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    await page.route(`**/api/play/sessions/${id}/mode`, async (route) => {
      await gate;
      Object.assign(state, { version: 2, tutor_threshold: 0.075 });
      await route.fulfill({ json: state });
    });
    await threshold.fill('0.075');
    await threshold.press('Tab');
    await expect(status).toHaveText(
      locale === 'de' ? 'Wird gespeichert …' : 'Saving…',
    );
    release();
    await expect(status).toHaveText(locale === 'de' ? 'Gespeichert' : 'Saved');
    await page.screenshot({
      path: `/tmp/bgtutor-threshold-status-${locale}.png`,
      fullPage: true,
    });
    await expect(status).toHaveText('', { timeout: 4500 });
  });
}

import type { PlaySession } from '../src/play-api';
import { watchOpening } from './play-opening-helpers';
import { backendTest as test, expect } from './test';

test.skip(
  !process.env.NEONDOJO_SAGE_PYTHON,
  'Requires locked local NeonGammon and GNUbg',
);

test('five-point difficulty labels and the selected opponent survive pause and reload', async ({
  page,
}, info) => {
  test.setTimeout(120_000);
  const installation = await (
    await page.request.get('/api/engines/bgsage')
  ).json();
  const verified = await page.request.put('/api/engines/bgsage', {
    data: {
      version: installation.version,
      path: process.env.NEONDOJO_SAGE_PYTHON,
    },
  });
  expect(verified.status(), await verified.text()).toBe(200);
  expect((await verified.json()).snapshot.package_version).toBe('3.1.0');

  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const workspace = page.getByRole('tabpanel', {
    name: 'Playing',
    exact: true,
  });
  const setup = workspace.getByRole('dialog', {
    name: 'New match',
    exact: true,
  });
  const strength = setup.getByLabel('Playing strength');
  await expect(strength.locator('option[value="beginner"]')).toHaveCount(0);
  await setup.getByLabel('Bot engine').selectOption('bgsage');
  const length = setup.getByLabel('Match length');
  await length.selectOption('5');
  await expect(
    strength.locator(
      'optgroup[label="Difficulty presets · 5-point PR"] option',
    ),
  ).toHaveText([
    'Beginner · ≈ 21 PR',
    'Casual · ≈ 14.4 PR',
    'Intermediate · ≈ 7.4 PR',
    'Full Strength · ≈ 0.9 PR',
  ]);
  await length.selectOption('7');
  await expect(strength.locator('option[value="beginner"]')).toHaveText(
    'Beginner',
  );
  await length.selectOption('5');
  await strength.selectOption('beginner');
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(strength).toBeInViewport();
    await setup.screenshot({
      path: info.outputPath(`difficulty-${width}.png`),
    });
  }
  await setup.getByRole('radio', { name: 'Competition', exact: true }).check();
  const opening = watchOpening(page);
  await setup.getByRole('button', { name: 'Start match', exact: true }).click();
  let session = await opening();
  expect(session.engine).toMatchObject({
    id: 'bgsage',
    plies: 1,
    difficulty: 'beginner',
  });
  expect(session.match_length).toBe(5);
  await expect(workspace.getByText('Beginner', { exact: true })).toBeVisible();
  const uri = `/api/play/sessions/${session.id}`;
  async function action(action: string, move?: number) {
    const response = await page.request.post(`${uri}/actions`, {
      data: { version: session.version, action, move },
    });
    expect(response.status(), await response.text()).toBe(200);
    session = (await response.json()) as PlaySession;
  }

  function botCheckerCount() {
    return session.history.filter(
      (turn) => turn.player !== session.human_player && turn.dice.length === 2,
    ).length;
  }

  async function completeBotTurn() {
    const previous = botCheckerCount();
    for (
      let attempt = 0;
      attempt < 5 && botCheckerCount() === previous;
      attempt++
    ) {
      switch (session.phase) {
        case 'roll':
          await action('roll');
          break;
        case 'move':
          if (session.legal_moves.length) await action('move', 0);
          else await action('pass');
          break;
        case 'double_response':
          await action('take');
          break;
        default:
          throw new Error(
            `Unexpected phase before the bot's next turn: ${session.phase}`,
          );
      }
    }
    expect(botCheckerCount()).toBeGreaterThan(previous);
  }

  if (!botCheckerCount()) await completeBotTurn();
  await action('pause');
  expect(session.paused).toBe(true);
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(workspace.getByText('Beginner', { exact: true })).toBeVisible();
  const restored = await (await page.request.get(uri)).json();
  expect(restored.engine).toEqual(session.engine);
  await action('resume');
  expect(session.paused).toBe(false);
  expect(session.engine).toEqual(restored.engine);
  await completeBotTurn();

  const rejected = await page.request.post('/api/play/sessions', {
    data: {
      human_player: 0,
      match_length: 5,
      play_engine: 'gnubg',
      play_difficulty: 'beginner',
    },
  });
  expect(rejected.status()).toBe(422);
});

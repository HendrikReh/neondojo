import {
  DESKTOP_VIEWPORTS,
  gotoWorkspace,
  openWorkspace,
} from '../helpers/navigation';
import releases from '../../../shared/neongammon-releases.json' with { type: 'json' };
import type { PlaySession } from '../../src/workspaces/play/play-api';
import { watchOpening } from '../helpers/play-opening-helpers';
import { backendTest as test, expect } from '../helpers/test';

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
  const { snapshot } = await verified.json();
  const presetVersions = Object.entries(releases)
    .filter(([, release]) => release.presets)
    .map(([version]) => version);
  expect(presetVersions).toContain(snapshot.package_version);

  await gotoWorkspace(page, 'play');
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
    'Beginner · ≈ 20.3 PR',
    'Casual · ≈ 15.8 PR',
    'Intermediate · ≈ 9.3 PR',
    'Full Strength · 3-ply checkers / 2-ply cube · ≈ 1.0 PR',
  ]);
  await length.selectOption('7');
  await expect(strength.locator('option[value="beginner"]')).toHaveText(
    'Beginner',
  );
  await expect(strength.locator('option[value="full_strength"]')).toHaveText(
    'Full Strength · 3-ply checkers / 2-ply cube',
  );
  await length.selectOption('5');
  await strength.selectOption('full_strength');
  for (const { width, height } of DESKTOP_VIEWPORTS) {
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
    plies: 3,
    difficulty: 'full_strength',
  });
  expect(session.match_length).toBe(5);
  await expect(
    workspace.getByText('Full Strength', { exact: true }),
  ).toBeVisible();
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
  await openWorkspace(page, 'play');
  await expect(
    workspace.getByText('Full Strength', { exact: true }),
  ).toBeVisible();
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

  const legacyRequest = await page.request.post('/api/play/sessions', {
    data: {
      human_player: 0,
      match_length: 5,
      play_engine: 'bgsage',
      play_plies: 2,
      play_difficulty: 'full_strength',
    },
  });
  expect(legacyRequest.status(), await legacyRequest.text()).toBe(201);
  expect((await legacyRequest.json()).engine).toMatchObject({
    plies: 3,
    difficulty: 'full_strength',
  });
});

import { expect, test } from './test';
import {
  openFullMatchAnalysis,
  openPositionCalculation,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { profileBackend } from './profile-backend';

test.use({ actionTimeout: 10_000 });

for (const confirmed of [false, true]) {
  test(`starts play and analysis when profiles fail with ${confirmed ? 'cached settings' : 'built-in defaults'}`, async ({
    page,
  }) => {
    if (confirmed) {
      const profile = await profileBackend(page);
      const config = profile.state.current_configuration;
      config.play.match_length = 3;
      config.analysis.plies = 1;
      await page.goto('/');
      await expect(
        page.getByRole('button', { name: /^Profile: Standard/ }),
      ).toBeVisible();
    }
    await page.route('**/api/configuration', (route) =>
      route.fulfill({
        status: 503,
        json: { code: 'profile_unavailable', error: 'Profile unavailable' },
      }),
    );
    if (!confirmed)
      await page.route('**/api/settings', (route) =>
        route.fulfill({
          status: 503,
          json: { error: 'Settings unavailable' },
        }),
      );
    if (confirmed) await page.reload();
    else await page.goto('/');
    await expect(
      page.getByText('Profile unavailable', { exact: true }),
    ).toBeVisible();

    await page.getByLabel('Match file', { exact: true }).setInputFiles({
      name: 'profile-fallback.sgf',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(
        `(;GM[6]PW[Fallback ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])`,
      ),
    });
    const position = await openPositionCalculation(page);
    const [job] = await Promise.all([
      page.waitForResponse(
        (r) =>
          r.request().method() === 'POST' &&
          new URL(r.url()).pathname === '/api/jobs',
        { timeout: 12_000 },
      ),
      position
        .getByRole('button', { name: 'Analyze position', exact: true })
        .click({ timeout: 10_000 }),
    ]);
    expect(job.status()).toBe(202);
    expect(job.request().postDataJSON()).toMatchObject({
      engine_id: 'gnubg',
      plies: confirmed ? 1 : 3,
    });
    await expect(
      page.locator(
        '.analysis-panel .candidates tbody tr[data-candidate-best="true"]',
      ),
    ).toBeVisible({ timeout: 15_000 });

    const entire = await submitFullMatchAnalysis(
      page,
      await openFullMatchAnalysis(page),
    );
    expect(entire.status()).toBe(202);
    expect(entire.request().postDataJSON()).toMatchObject({
      engine_id: 'gnubg',
      plies: confirmed ? 1 : 3,
    });

    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const setup = page.locator('.play-setup');
    const [game] = await Promise.all([
      page.waitForResponse(
        (r) =>
          r.request().method() === 'POST' &&
          new URL(r.url()).pathname === '/api/play/sessions',
        { timeout: 12_000 },
      ),
      setup
        .getByRole('button', { name: 'Start match', exact: true })
        .click({ timeout: 10_000 }),
    ]);
    expect(game.status()).toBe(201);
    expect(game.request().postDataJSON()).toMatchObject({
      match_length: confirmed ? 3 : 7,
    });
    await expect(page.locator('.play-match-heading')).toContainText(
      confirmed ? '3 points' : '7 points',
    );
  });
}

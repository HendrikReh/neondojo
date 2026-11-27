import { test, expect } from '../helpers/test';
import { historyFixture } from '../helpers/play-history-review-helpers';
test('rapid selection, cancelled fetch and late response never change live play', async ({
  page,
}) => {
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let calls = 0;
  const f = await historyFixture(page, 0, async (route, data) => {
    if (++calls === 1) await gate;
    await route.fulfill({ json: data }).catch(() => {});
  });
  try {
    await f.rows.first().click();
    await expect(
      page.locator('.play-history-review [role="status"]'),
    ).toBeVisible();
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '1',
    );
    expect(f.readCount()).toBe(1);
    await expect(
      page.locator('[data-play-point][data-enabled="true"]'),
    ).toHaveCount(0);
    await page
      .getByRole('button', { name: 'Return to game', exact: true })
      .click();
    await expect(page.locator('.play-history-review')).toHaveCount(0);
    await f.rows.last().click();
    await expect(
      page.locator('.play-history-review [role="status"]'),
    ).toHaveCount(0);
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '1',
    );
    expect(f.readCount()).toBe(2);
    release();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '1',
    );
    expect(f.writes).toEqual([]);
  } finally {
    release();
  }
});
for (const failure of ['conflict', 'version', 'index'])
  test(`history ${failure} fails closed and return restores live controls`, async ({
    page,
  }) => {
    const f = await historyFixture(page, 0, async (route, data) =>
      route.fulfill(
        failure === 'conflict'
          ? { status: 409, json: { error: 'Session changed; refresh history' } }
          : {
              json:
                failure === 'version'
                  ? { ...data, version: 8 }
                  : {
                      ...data,
                      items: data.items.map((x) => ({ ...x, index: 99 })),
                    },
            },
      ),
    );
    await f.rows.first().click();
    await expect(
      page.locator('.play-history-review [role="alert"]'),
    ).toBeVisible();
    await expect(
      page.locator('[data-play-point][data-enabled="true"]'),
    ).toHaveCount(0);
    await page
      .getByRole('button', { name: 'Return to game', exact: true })
      .click();
    await expect(page.locator('.play-history-review')).toHaveCount(0);
    await expect(
      page.locator('[data-play-point][data-enabled="true"]').first(),
    ).toBeVisible();
    expect(f.writes).toEqual([]);
  });

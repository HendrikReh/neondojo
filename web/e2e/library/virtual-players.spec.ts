import { openWorkspace, seedLocale } from '../helpers/navigation';
import { importSgfMatch } from '../helpers/match-fixtures';
import { expect, test, type Page } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { applyAndCloseSettings } from '../helpers/settings-actions';

const importMatch = (page: Page, sgf: string) =>
  importSgfMatch(page, { filename: 'virtual.sgf', sgf });

async function cleanUp(page: Page, name: string) {
  await page.request.put('/api/virtual-players/me', { data: { id: null } });
  const list = await page.request.get('/api/virtual-players');
  const { items } = (await list.json()) as {
    items: { id: string; name: string; version: number }[];
  };
  for (const item of items.filter((row) => row.name === name))
    await page.request.delete(
      `/api/virtual-players/${item.id}?version=${item.version}`,
    );
}

test('virtual profiles group members, consolidate results and stand for me in Study', async ({
  page,
}) => {
  await profileBackend(page);
  await seedLocale(page);
  await page.setViewportSize({ width: 1800, height: 1169 });
  const token = crypto.randomUUID().slice(0, 8);
  const alias = `acct${token}`;
  const group = `Person ${token}`;
  await importMatch(
    page,
    `(;GM[6]PW[${alias}]PB[Galaxy rival ${token}]PC[BackgammonGalaxy]MI[length:7];W[31hefe])`,
  );
  await importMatch(
    page,
    `(;GM[6]PW[Open rival ${token}]PB[${alias}]PC[OpenGammon]MI[length:5];W[31hefe])`,
  );
  // The saved rating summary is computed by the backend tests; here the card
  // must show the consolidated PR and its trend.
  await page.route(/\/api\/virtual-players\/[0-9a-f-]{36}$/, async (route) => {
    const response = await route.fetch();
    if (route.request().method() !== 'GET') return route.fulfill({ response });
    const details = await response.json();
    if (details.member_count === 2) {
      details.ratings = [
        {
          profile: 'GNUbg; 2-ply cubeful deterministic',
          value: 4.2,
          match_count: 2,
        },
      ];
      details.rating_history = [
        ['2026-01-02', 3.1],
        ['2026-02-03', 5.3],
      ].map(([date, value], index) => ({
        match_id: `00000000-0000-4000-8000-00000000000${index}`,
        date,
        profile: 'GNUbg; 2-ply cubeful deterministic',
        value,
        decisions: 40,
      }));
    }
    return route.fulfill({ response, json: details });
  });
  try {
    await page.goto('/');
    await page.getByRole('tab', { name: 'Player database' }).click();
    const strip = page.getByRole('region', { name: 'Virtual profiles' });
    await strip.getByRole('button', { name: 'New virtual profile …' }).click();
    const create = page.getByRole('dialog', { name: 'New virtual profile' });
    await create.getByRole('textbox', { name: 'Name' }).fill(group);
    await create.getByRole('button', { name: 'Create' }).click();
    await expect(create).toBeHidden();
    const card = page.locator('.virtual-player-card');
    await expect(card.getByRole('heading', { name: group })).toBeVisible();
    await expect(card).toContainText('0 members · 0 matches · 0 games');
    await expect(
      strip.getByRole('button', { name: group, pressed: true }),
    ).toBeVisible();

    // The two accounts share a name; each row is one platform's profile.
    for (const row of [0, 1]) {
      const trigger = card.getByRole('button', { name: 'Add member …' });
      await trigger.click();
      const dialog = page.getByRole('dialog', { name: 'Add member' });
      await expect(dialog.getByRole('searchbox')).toBeFocused();
      await dialog.getByRole('searchbox').fill(alias);
      await dialog
        .locator('.player-result-table tbody tr')
        .nth(row)
        .getByRole('button', { name: alias })
        .click();
      await expect(dialog.getByText('1 match · 1 game')).toBeVisible();
      if (row === 1)
        await dialog.screenshot({
          path: test.info().outputPath('virtual-player-member-dialog.png'),
        });
      await dialog.getByRole('button', { name: 'Add', exact: true }).click();
      await expect(dialog).toBeHidden();
      await expect(trigger).toBeFocused();
    }
    await expect(card).toContainText('2 members · 2 matches · 2 games');
    const members = card.getByRole('region', { name: 'Members' });
    for (const platform of ['backgammongalaxy.com', 'opengammon.com'])
      await expect(members).toContainText(`${alias} · ${platform}`);

    // Consolidated PR, matches from both sites and the trend.
    const ratings = card.getByRole('region', {
      name: 'Performance rating (PR)',
    });
    await expect(ratings).toContainText('4.2');
    await expect(ratings).toContainText('2 of 2 matches rated');
    const matches = card.getByRole('region', { name: 'Matches' });
    await expect(matches.getByRole('heading').locator('span')).toHaveText('2');
    for (const rival of [`Galaxy rival ${token}`, `Open rival ${token}`])
      await expect(matches.getByRole('cell', { name: rival })).toBeVisible();
    await expect(
      card.getByRole('region', { name: 'PR over time' }),
    ).toBeVisible();
    for (const [width, height] of [
      [1800, 1169],
      [2560, 1440],
    ]) {
      await page.setViewportSize({ width, height });
      for (const box of [card, card.locator('.player-rating-trends')]) {
        expect(
          await box.evaluate(
            (element) => element.scrollWidth - element.clientWidth,
          ),
        ).toBe(0);
      }
      await card.screenshot({
        path: test.info().outputPath(`virtual-player-card-${width}.png`),
      });
    }

    // Members stay in the player list and name their group.
    await page.getByRole('searchbox', { name: 'Search players' }).fill(alias);
    const listed = page.locator('.player-list .player-result-table tbody tr');
    await expect(listed).toHaveCount(2);
    await expect(listed.locator('.player-group-hit')).toContainText([
      group,
      group,
    ]);

    // Removing a member leaves the real profile in the list.
    await members
      .getByRole('listitem')
      .filter({ hasText: 'opengammon.com' })
      .getByRole('button', { name: 'Remove' })
      .click();
    await expect(card).toContainText('1 member · 1 match · 1 game');
    await expect(listed).toHaveCount(2);
    await expect(listed.locator('.player-group-hit')).toHaveCount(1);
    await listed.first().getByRole('button', { name: alias }).click();
    await expect(page.locator('.player-card').first()).toBeVisible();
    const partOf = page.locator('.player-card-group');
    if (await partOf.count()) {
      await partOf.getByRole('button', { name: group }).click();
      await expect(card.getByRole('heading', { name: group })).toBeVisible();
    }

    // "My player profile" in Settings makes the group the default player.
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    const settings = page.getByRole('dialog', {
      name: 'Settings',
      exact: true,
    });
    const me = settings.getByRole('combobox', {
      name: 'My player profile',
      exact: true,
    });
    await expect(me).toContainText(group);
    await me.selectOption({ label: group });
    await expect(
      settings.getByRole('button', { name: 'Apply', exact: true }),
    ).toHaveAccessibleDescription(
      'My player profile is saved with the player database.',
    );
    await applyAndCloseSettings(settings);
    await expect(
      strip.getByRole('button', { name: group }).locator('.virtual-player-me'),
    ).toHaveText('My profile');

    await openWorkspace(page, 'study');
    const overview = page.getByRole('tabpanel', {
      name: 'Overview',
      exact: true,
    });
    await expect(overview.getByLabel('Player', { exact: true })).toHaveValue(
      group,
    );

    await openWorkspace(page, 'positions');
    await page
      .getByRole('group', { name: 'Position source' })
      .getByRole('button', { name: 'All analyzed decisions' })
      .click();
    const search = page.getByRole('region', { name: 'Analyzed decisions' });
    await expect(search.getByLabel('Player', { exact: true })).toHaveValue(
      group,
    );

    // The player database opens "me" first.
    await page.reload();
    await page.getByRole('tab', { name: 'Player database' }).click();
    await expect(card.getByRole('heading', { name: group })).toBeVisible();
    await expect(card).toContainText('My profile');
    await expect(card).toContainText(
      'Your NeonDojo bot matches count once your NeonDojo player is a member.',
    );
  } finally {
    await cleanUp(page, group);
  }
});

test('a virtual profile with a taken name is rejected and can be renamed and deleted', async ({
  page,
}) => {
  // The unconfigured profile keeps the locale chosen in the browser.
  await seedLocale(page, 'de');
  const token = crypto.randomUUID().slice(0, 8);
  const group = `Gruppe ${token}`;
  try {
    await page.goto('/');
    await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
    const strip = page.getByRole('region', { name: 'Virtuelle Profile' });
    for (const attempt of [0, 1]) {
      await strip
        .getByRole('button', { name: 'Neues virtuelles Profil …' })
        .click();
      const create = page.getByRole('dialog', {
        name: 'Neues virtuelles Profil',
      });
      await create.getByRole('textbox', { name: 'Name' }).fill(group);
      await create.getByRole('button', { name: 'Anlegen' }).click();
      if (attempt === 0) await expect(create).toBeHidden();
      else {
        await expect(create.getByRole('alert')).toContainText(
          'Ein virtuelles Profil mit diesem Namen gibt es bereits.',
        );
        await create.getByRole('button', { name: 'Abbrechen' }).click();
      }
    }
    const card = page.locator('.virtual-player-card');
    await expect(card).toContainText('0 Mitglieder');
    await expect(card).toContainText('Noch keine Mitglieder.');
    await card.getByRole('button', { name: 'Umbenennen …' }).click();
    const rename = page.getByRole('dialog', {
      name: 'Virtuelles Profil umbenennen',
    });
    await rename.getByRole('textbox', { name: 'Name' }).fill(`${group} B`);
    await rename.getByRole('button', { name: 'Speichern' }).click();
    await expect(
      card.getByRole('heading', { name: `${group} B` }),
    ).toBeVisible();
    await expect(
      strip.getByRole('button', { name: `${group} B` }),
    ).toBeVisible();
    await card.getByRole('button', { name: 'Löschen …' }).click();
    const remove = page.getByRole('dialog', {
      name: 'Virtuelles Profil löschen',
    });
    await expect(remove).toContainText(
      `„${group} B“ wird gelöscht. Seine Mitgliedsprofile, Matches und Analysen bleiben unverändert.`,
    );
    await remove
      .getByRole('button', { name: 'Virtuelles Profil löschen' })
      .click();
    await expect(remove).toBeHidden();
    await expect(strip.getByRole('button', { name: `${group} B` })).toHaveCount(
      0,
    );
  } finally {
    await cleanUp(page, group);
    await cleanUp(page, `${group} B`);
  }
});

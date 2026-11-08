import { test, expect } from '@playwright/test';

async function importStudy(
  page: import('@playwright/test').Page,
  suffix: string,
  metadata = 'MI[length:7][ws:0][bs:0]',
) {
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({
    name: `study-${suffix}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Study ${suffix}]PB[Opponent]${metadata};W[31xufe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.board')).toBeVisible();
}

test('match and position notes persist, search jumps to the exact move, and conflicts retain the draft', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  await importStudy(page, suffix);
  const match = page.locator('[data-annotation-kind=match]');
  await match.getByLabel('Tags', { exact: true }).fill(`Match-${suffix}`);
  await match
    .getByRole('button', { name: 'Tag hinzufügen', exact: true })
    .click();
  await match
    .getByLabel('Kommentar', { exact: true })
    .fill(`match-comment-${suffix}`);
  await match
    .getByRole('button', { name: 'Notizen speichern', exact: true })
    .click();
  await expect(
    match.getByText('Notizen gespeichert.', { exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Nächster Zug', exact: true }).click();
  const position = page.locator('[data-annotation-kind=position]');
  await position.getByLabel('Tags', { exact: true }).fill(`Position-${suffix}`);
  await position
    .getByLabel('Kommentar', { exact: true })
    .fill(`position-comment-${suffix} <img src=x onerror=alert(1)>`);
  await position
    .getByRole('button', { name: 'Notizen speichern', exact: true })
    .click();
  await expect(
    position.getByText('Notizen gespeichert.', { exact: true }),
  ).toBeVisible();
  await page.reload();
  await page
    .locator('.match-card')
    .filter({ hasText: `Study ${suffix}` })
    .click();
  await expect(match.getByLabel('Kommentar', { exact: true })).toHaveValue(
    `match-comment-${suffix}`,
  );
  const search = page.locator('.annotation-search');
  await search.locator('summary').click();
  await search
    .getByLabel('Text', { exact: true })
    .fill(`position-comment-${suffix}`);
  await search
    .getByRole('button', { name: 'Notizen suchen', exact: true })
    .click();
  await expect(search.locator('.annotation-search-hit')).toHaveCount(1);
  await search
    .getByRole('button', { name: 'Position öffnen', exact: true })
    .click();
  await expect(page.locator('.board-heading')).toContainText('Zug 2');
  await expect(position.getByLabel('Kommentar', { exact: true })).toHaveValue(
    `position-comment-${suffix} <img src=x onerror=alert(1)>`,
  );
  const id = await position.getAttribute('data-annotation-id');
  const current = await (
    await page.request.get(`/api/positions/${id}/annotation`)
  ).json();
  const external = await page.request.put(`/api/positions/${id}/annotation`, {
    data: {
      tags: ['external'],
      comment: 'External version',
      expected_version: current.version,
    },
  });
  expect(external.status()).toBe(200);
  await position
    .getByLabel('Kommentar', { exact: true })
    .fill('My retained draft');
  await position
    .getByRole('button', { name: 'Notizen speichern', exact: true })
    .click();
  await expect(position.locator('.annotation-conflict')).toBeVisible();
  await expect(position.getByLabel('Kommentar', { exact: true })).toHaveValue(
    'My retained draft',
  );
  await position
    .getByRole('button', {
      name: 'Neueste Version laden und Entwurf verwerfen',
    })
    .click();
  await expect(position.getByLabel('Kommentar', { exact: true })).toHaveValue(
    'External version',
  );
});

async function unusedLearningScores(page: import('@playwright/test').Page) {
  const used = new Set<string>();
  const decks = await (await page.request.get('/api/decks')).json();
  for (const deck of decks) {
    let after: string | null = null;
    do {
      const data = await (
        await page.request.get(
          `/api/decks/${deck.id}/cards?limit=100${after ? `&after=${after}` : ''}`,
        )
      ).json();
      for (const card of data.items) {
        const bytes = Buffer.from(card.position.gnubg_match_id, 'base64');
        const bits = (start: number, n: number) =>
          Array.from(
            { length: n },
            (_, i) =>
              ((bytes[(start + i) >> 3] >> ((start + i) % 8)) & 1) * 2 ** i,
          ).reduce((a, b) => a + b, 0);
        if (bits(21, 15) === 25) used.add(`${bits(36, 15)}:${bits(51, 15)}`);
      }
      after = data.next_cursor;
    } while (after);
  }
  for (let white = 0; white < 24; white++)
    for (let black = 0; black < 24; black++)
      if (!used.has(`${white}:${black}`))
        return `MI[length:25][ws:${white}][bs:${black}]`;
  throw new Error('No unused study context in fixture range');
}

test('an analyzed annotated position becomes a shared study card and a downloadable Anki deck', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  const metadata = await unusedLearningScores(page);
  await importStudy(page, suffix, metadata);
  const failures: string[] = [];
  page.on('pageerror', (error) => failures.push(error.message));
  const otherDeck = await (
    await page.request.post('/api/decks', {
      data: { name: `Existing ${suffix}` },
    })
  ).json();
  await page.reload();
  await page
    .locator('.match-card')
    .filter({ hasText: `Study ${suffix}` })
    .click();
  const position = page.locator('[data-annotation-kind=position]');
  const note = `lesson-${suffix}`;
  await position.getByLabel('Tags', { exact: true }).fill('Opening plans');
  await position.getByLabel('Kommentar', { exact: true }).fill(note);
  await position
    .getByRole('button', { name: 'Notizen speichern', exact: true })
    .click();
  await expect(
    position.getByText('Notizen gespeichert.', { exact: true }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: 'Position analysieren', exact: true })
    .click();
  await expect(page.getByText('BESTER ZUG', { exact: true })).toBeVisible({
    timeout: 30_000,
  });
  const add = page.getByRole('region', {
    name: 'Positionen als Lernkarten',
    exact: true,
  });
  await add.getByLabel('Deckname', { exact: true }).fill(`Practice ${suffix}`);
  const createdResponse = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === '/api/decks',
  );
  await add
    .getByRole('button', { name: 'Deck erstellen', exact: true })
    .click();
  const deck = await (await createdResponse).json();
  await expect(add.getByLabel('Deck auswählen', { exact: true })).toHaveValue(
    deck.id,
  );
  await add
    .getByRole('button', { name: 'Aktuelle Position hinzufügen', exact: true })
    .click();
  await expect(add).toContainText('1 Karten verarbeitet.');
  await page
    .getByRole('button', { name: 'Lernen & Anki', exact: true })
    .click();
  const study = page.getByRole('region', {
    name: 'Lernen & Anki',
    exact: true,
  });
  await study
    .getByLabel('Deck auswählen', { exact: true })
    .selectOption(deck.id);
  await study
    .getByRole('button', { name: 'Jetzt lernen', exact: true })
    .click();
  await expect(study.locator('.study-card')).toBeVisible();
  await expect(study.locator('.study-answer')).toHaveCount(0);
  await expect(study.getByText(note, { exact: false })).toHaveCount(0);
  let releaseReveal: () => void = () => {};
  let signalReveal: () => void = () => {};
  const revealGate = new Promise<void>((resolve) => {
    releaseReveal = resolve;
  });
  const revealStarted = new Promise<void>((resolve) => {
    signalReveal = resolve;
  });
  await page.route('**/api/cards/*/reveal', async (route) => {
    signalReveal();
    await revealGate;
    await route.continue();
  });
  const oldReveal = page.waitForResponse(
    (r) => r.request().method() === 'POST' && r.url().endsWith('/reveal'),
  );
  await study
    .getByRole('button', { name: 'Antwort aufdecken', exact: true })
    .click();
  await revealStarted;
  try {
    await expect(
      study.getByRole('button', { name: 'Jetzt lernen', exact: true }),
    ).toBeDisabled();
    await study
      .getByRole('button', { name: 'Zurück zu den Decks', exact: true })
      .click();
    await study
      .getByRole('button', { name: 'Jetzt lernen', exact: true })
      .click();
    await expect(study.locator('.study-card')).toBeVisible();
    await expect(
      study.getByRole('button', { name: 'Antwort aufdecken', exact: true }),
    ).toBeEnabled();
  } finally {
    releaseReveal();
  }
  await (await oldReveal).finished();
  await page.unroute('**/api/cards/*/reveal');
  await study
    .getByRole('button', { name: 'Antwort aufdecken', exact: true })
    .click();
  await expect(study.locator('.study-answer')).toContainText('8/5 6/5');
  await expect(study.locator('.study-answer')).toContainText(note);
  await study.getByRole('button', { name: /^Gut/ }).click();
  await expect(
    study.getByText('Für dieses Deck sind gerade keine Karten fällig.', {
      exact: true,
    }),
  ).toBeVisible();
  await expect(study.locator('.learning-review-result')).toContainText(
    'Nächste Wiederholung',
  );
  let releaseExport: () => void = () => {};
  let signalExport: () => void = () => {};
  const exportGate = new Promise<void>((resolve) => {
    releaseExport = resolve;
  });
  const exportStarted = new Promise<void>((resolve) => {
    signalExport = resolve;
  });
  await page.route('**/api/decks/*/export.apkg?**', async (route) => {
    signalExport();
    await exportGate;
    await route.continue();
  });
  const download = page.waitForEvent('download');
  await study
    .getByRole('button', { name: 'Anki-Deck herunterladen', exact: true })
    .click();
  await exportStarted;
  await study
    .getByLabel('Deck auswählen', { exact: true })
    .selectOption(otherDeck.id);
  releaseExport();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.apkg$/);
  await file.saveAs('test-results/study-export.apkg');
  await page.unroute('**/api/decks/*/export.apkg?**');
  await study
    .getByLabel('Deck auswählen', { exact: true })
    .selectOption(deck.id);
  await expect(
    study.getByRole('button', { name: 'Anki-Deck herunterladen', exact: true }),
  ).toBeEnabled();
  await page.reload();
  await page
    .getByRole('button', { name: 'Lernen & Anki', exact: true })
    .click();
  await study
    .getByLabel('Deck auswählen', { exact: true })
    .selectOption(deck.id);
  await study
    .getByRole('button', { name: 'Jetzt lernen', exact: true })
    .click();
  await expect(
    study.getByText('Für dieses Deck sind gerade keine Karten fällig.', {
      exact: true,
    }),
  ).toBeVisible();
  expect(failures).toEqual([]);
});

test('an annotation jump cancels an older move-page navigation and releases navigation controls', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .locator('input[type=file]')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  await expect(page.locator('.moves-list button')).toHaveCount(100);
  const id = await page
    .locator('[data-annotation-kind=position]')
    .getAttribute('data-annotation-id');
  const current = await (
    await page.request.get(`/api/positions/${id}/annotation`)
  ).json();
  const marker = `navigation-${crypto.randomUUID()}`;
  expect(
    (
      await page.request.put(`/api/positions/${id}/annotation`, {
        data: {
          comment: marker,
          tags: ['navigation'],
          expected_version: current.version,
        },
      })
    ).status(),
  ).toBe(200);
  const search = page.locator('.annotation-search');
  await search.locator('summary').click();
  await search.getByLabel('Text', { exact: true }).fill(marker);
  await search
    .getByRole('button', { name: 'Notizen suchen', exact: true })
    .click();
  await expect(
    search.getByRole('button', { name: 'Position öffnen', exact: true }),
  ).toBeVisible();
  let release: () => void = () => {};
  let requested: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  await page.route('**/api/positions?**', async (route) => {
    if (new URL(route.request().url()).searchParams.has('after')) {
      requested();
      await gate;
    }
    await route.continue();
  });
  const response = page.waitForResponse(
    (r) =>
      r.url().includes('/api/positions?') &&
      new URL(r.url()).searchParams.has('after'),
  );
  await page.getByRole('button', { name: 'Weiter im Verlauf' }).click();
  await started;
  try {
    await search
      .getByRole('button', { name: 'Position öffnen', exact: true })
      .click();
    await expect(
      page.getByRole('button', { name: 'Nächster Zug', exact: true }),
    ).toBeEnabled();
  } finally {
    release();
  }
  await (await response).finished();
  await page.waitForTimeout(100);
  await expect(page.locator('.board-navigation')).toContainText('1 / 460');
  await page.unroute('**/api/positions?**');

  let releaseView: () => void = () => {};
  let signalView: () => void = () => {};
  const viewGate = new Promise<void>((resolve) => {
    releaseView = resolve;
  });
  const viewStarted = new Promise<void>((resolve) => {
    signalView = resolve;
  });
  await page.route('**/api/positions?**', async (route) => {
    if (new URL(route.request().url()).searchParams.has('after')) {
      signalView();
      await viewGate;
    }
    await route.continue();
  });
  const oldPage = page.waitForResponse(
    (r) =>
      r.url().includes('/api/positions?') &&
      new URL(r.url()).searchParams.has('after'),
  );
  await page.getByRole('button', { name: 'Weiter im Verlauf' }).click();
  await viewStarted;
  await page
    .getByRole('button', { name: 'Lernen & Anki', exact: true })
    .click();
  releaseView();
  await (await oldPage).finished();
  await page.getByRole('button', { name: 'Bibliothek', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Nächster Zug', exact: true }),
  ).toBeEnabled();
  await expect(page.locator('.board-navigation')).toContainText('1 / 460');
});

test('a whole-match deck trains checker, double and response cards with the selected theme', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  const metadata = await unusedLearningScores(page);
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({
    name: `bulk-${suffix}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Bulk ${suffix}]PB[Opponent]${metadata};W[31hefe])(;GM[6]${metadata}AE[a:y]AW[f]AB[s]PL[W];W[double];B[drop])`,
    ),
  });
  await expect(page.locator('.board')).toBeVisible();
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
    exact: true,
  });
  const started = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      /\/api\/matches\/[^/]+\/analysis$/.test(new URL(r.url()).pathname),
  );
  await panel
    .getByRole('button', {
      name: /Gesamte Partie analysieren|Partie erneut analysieren/,
    })
    .click();
  const job = await (await started).json();
  await expect(panel).toHaveAttribute('data-job-id', job.id);
  await expect(
    panel.getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 30_000 });
  const add = page.getByRole('region', {
    name: 'Positionen als Lernkarten',
    exact: true,
  });
  await add.getByLabel('Deckname', { exact: true }).fill(`Bulk ${suffix}`);
  const created = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === '/api/decks',
  );
  await add
    .getByRole('button', { name: 'Deck erstellen', exact: true })
    .click();
  const deck = await (await created).json();
  await expect(add.getByLabel('Deck auswählen', { exact: true })).toHaveValue(
    deck.id,
  );
  await add.getByLabel('Mindestverlust', { exact: true }).fill('0');
  await add
    .getByRole('button', { name: 'Fehler aus Partie übernehmen', exact: true })
    .click();
  await expect(add).toContainText('3 Karten verarbeitet.');
  if (
    await page
      .getByLabel('Board-Theme')
      .locator('option[value="bgblitz:bg-earthboard"]')
      .count()
  )
    await page.getByLabel('Board-Theme').selectOption('bgblitz:bg-earthboard');
  await page
    .getByRole('button', { name: 'Lernen & Anki', exact: true })
    .click();
  const study = page.getByRole('region', {
    name: 'Lernen & Anki',
    exact: true,
  });
  await study
    .getByLabel('Deck auswählen', { exact: true })
    .selectOption(deck.id);
  await study
    .getByRole('button', { name: 'Jetzt lernen', exact: true })
    .click();
  let cubeCards = 0;
  for (let index = 0; index < 3; index++) {
    const card = study.locator('.study-card');
    await expect(card).toBeVisible();
    await expect(card.locator('.study-context')).toContainText(
      '25-Punkte-Match',
    );
    await expect(card.locator('.study-context')).toContainText('am Zug');
    const id = await card.getAttribute('data-card-id');
    if (
      !(await card
        .getByRole('heading', {
          name: 'Welchen Zug würdest du spielen?',
          exact: true,
        })
        .count())
    ) {
      cubeCards++;
      await expect(card.locator('[data-dice] image')).toHaveCount(0);
      if (
        await card
          .getByRole('heading', { name: 'Annehmen oder passen?', exact: true })
          .count()
      )
        await expect(card).toContainText('Angebotener Doppler: 2');
    }
    await card
      .getByRole('button', { name: 'Antwort aufdecken', exact: true })
      .click();
    await expect(card.locator('.study-answer')).toBeVisible();
    await card.screenshot({ path: 'test-results/learning-study-answer.png' });
    const graded = page.waitForResponse(
      (r) =>
        r.request().method() === 'POST' &&
        r.url().endsWith(`/cards/${id}/review`),
    );
    await card.getByRole('button', { name: /^Gut/ }).click();
    await (await graded).finished();
    await expect(
      study.locator(`.study-card[data-card-id="${id}"]`),
    ).toHaveCount(0);
  }
  expect(cubeCards).toBe(2);
  await expect(
    study.getByText('Für dieses Deck sind gerade keine Karten fällig.', {
      exact: true,
    }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

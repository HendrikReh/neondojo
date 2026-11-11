import { test, expect } from './test';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';
import { applySteps } from '../src/play-state';
import { nativeSteps } from '../src/play-sound-events';

for (const { width, human, locale } of [
  { width: 1440, human: 0, locale: 'en' },
  { width: 390, human: 0, locale: 'en' },
  { width: 1440, human: 1, locale: 'de' },
  { width: 390, human: 1, locale: 'de' },
])
  test(`board stays in place through Coach, bot playback and the next roll at ${width}/${locale}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    const { saved, point } = await historyFixture(page, human);
    Object.assign(saved, { play_mode: 'coach' });
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    let writes = 0;
    await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
      writes++;
      if (writes === 1) {
        await gate;
        Object.assign(saved, {
          version: 8,
          pending_decision: {
            id,
            action: 'move',
            original_action: 'move',
            move_index: 0,
            original_move_index: 0,
            notation: '13/10 6/5',
            warning: false,
            revealed: true,
            assisted: false,
            retry_count: 0,
            alternatives: [],
            attempts: [],
            committed: false,
          },
        });
      } else if (writes === 2) {
        const humanTurn = {
          player: human,
          dice: [3, 1],
          notation: '13/10 6/5',
        };
        const bot = { player: 1 - human, dice: [3, 1], notation: '13/10 6/5' };
        for (const turn of [humanTurn, bot]) {
          saved.position.board = applySteps(
            saved.position.board,
            nativeSteps(turn.notation, turn.player),
            turn.player,
          );
          saved.history.push(turn);
        }
        Object.assign(saved, {
          version: 9,
          phase: 'roll',
          pending_decision: null,
        });
        saved.position.dice = [];
      } else {
        Object.assign(saved, { version: 10, phase: 'move' });
        saved.position.dice = [3, 1];
      }
      await route.fulfill({ json: saved });
    });
    await page.reload();
    await page.locator('#workspace-tab-play').click();
    if (locale === 'de') {
      await page.getByRole('button', { name: 'Settings', exact: true }).click();
      const settings = page.getByRole('dialog', {
        name: 'Settings',
        exact: true,
      });
      await settings
        .getByRole('button', { name: 'General', exact: true })
        .click();
      await settings.getByLabel('Language', { exact: true }).selectOption('de');
      await settings
        .getByRole('button', { name: 'Apply', exact: true })
        .click();
      await expect(page.locator('html')).toHaveAttribute('lang', 'de');
    }
    const board = page.locator('.play-live-board > .board-shell > .board');
    await expect(board).toBeVisible();
    await board.evaluate((el) => {
      window.scrollBy(0, el.getBoundingClientRect().top - 80);
    });
    const baseline = await board.boundingBox();
    const scrollY = await page.evaluate(() => window.scrollY);
    const stable = async () => {
      const box = await board.boundingBox();
      expect(box!.y).toBeCloseTo(baseline!.y, 0);
      expect(box!.height).toBeCloseTo(baseline!.height, 0);
      expect(await page.evaluate(() => window.scrollY)).toBeCloseTo(scrollY, 0);
    };
    try {
      await point(13).click();
      await point(6).click();
      await page.locator('[data-dice-action="confirm"]').first().click();
      await expect(page.locator('.play-thinking')).toBeVisible();
      await stable();
      release();
      const coach = page.getByRole('alertdialog', {
        name: 'Coach',
        exact: true,
      });
      await expect(coach).toBeVisible();
      expect(await coach.evaluate((el) => el.matches(':modal'))).toBe(true);
      await stable();
      await coach
        .getByRole('button', {
          name: locale === 'de' ? 'Weiter' : 'Continue',
          exact: true,
        })
        .click();
      await expect(page.locator('.play-live-board')).toHaveAttribute(
        'data-play-animation',
        'dice',
      );
      await stable();
      const next = page.getByRole('alertdialog', {
        name: locale === 'de' ? 'Nächster Wurf' : 'Next roll',
        exact: true,
      });
      await expect(next).toBeVisible();
      await stable();
      await next
        .getByRole('button', {
          name: locale === 'de' ? 'Würfeln' : 'Roll dice',
          exact: true,
        })
        .click();
      await expect(page.getByTestId('play-board-toolbar')).toBeVisible();
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'false',
      );
      await stable();
    } finally {
      release();
    }
  });

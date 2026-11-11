import type { Page } from '@playwright/test';
export const rollDice = (page: Page) =>
  page.locator('.play-board [data-play-action="roll"]').first();
export const doubleCube = (page: Page) =>
  page.locator('.play-board [data-play-action="double"]');

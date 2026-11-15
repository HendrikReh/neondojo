import type { Page } from '@playwright/test';
export const botDice = (page: Page) =>
  page.locator('.play-board [data-dice-action="roll"]');
export const rollDice = (page: Page) =>
  page
    .locator(
      '.play-board [data-play-action="roll"]:enabled, .play-board [data-dice-action="roll"]',
    )
    .first();
export const doubleCube = (page: Page) =>
  page.locator('.play-board [data-cube][role="button"]');
export const passDice = (page: Page) =>
  page.locator('.play-board [data-dice-action="pass"]');

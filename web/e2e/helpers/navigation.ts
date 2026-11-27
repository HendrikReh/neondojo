import { expect, type Locator, type Page } from '@playwright/test';

export const DESKTOP_VIEWPORTS = [
  { width: 1800, height: 1169 },
  { width: 2560, height: 1440 },
  { width: 3840, height: 2160 },
] as const;

export type DesktopViewport = (typeof DESKTOP_VIEWPORTS)[number];

export type WorkspaceTab =
  'analysis' | 'play' | 'study' | 'positions' | 'library' | 'extras';

const WORKSPACE_TAB_NAMES: Record<WorkspaceTab, RegExp> = {
  analysis: /^(Analysis|Analyse)$/,
  play: /^(Playing|Spielen)$/,
  study: /^(Study|Lernen)$/,
  positions: /^(Positions|Positionen)$/,
  library: /^(Match database|Matchdatenbank)$/,
  extras: /^Extras$/,
};

export interface SeedStorageOptions {
  locale?: 'en' | 'de';
  /** Only write `neondojo.locale` when it is not already set (e.g. reload tests). */
  preserveLocale?: boolean;
  databaseId?: string;
  deckId?: string;
  playSessionId?: string;
  playSounds?: 'on' | 'off';
  theme?: string;
  uiAppearance?: { version?: number; theme: string; mode: 'light' | 'dark' };
  workspace?: {
    matchId: string;
    gameNo?: number;
    positionId?: string;
  };
  extra?: Record<string, string | null | undefined>;
}

/**
 * Seeds NeonDojo `localStorage` keys before initial page load via `page.addInitScript`.
 */
export async function seedStorage(
  page: Page,
  options: SeedStorageOptions = {},
) {
  const entries: Record<string, string> = {};
  if (options.databaseId) entries['neondojo.database'] = options.databaseId;
  if (options.deckId) entries['neondojo.deck'] = options.deckId;
  if (options.playSessionId)
    entries['neondojo.play-session'] = options.playSessionId;
  if (options.playSounds) entries['neondojo.play-sounds'] = options.playSounds;
  if (options.theme) entries['neondojo.theme'] = options.theme;
  if (options.uiAppearance) {
    entries['neondojo.ui-appearance'] = JSON.stringify({
      version: options.uiAppearance.version ?? 1,
      theme: options.uiAppearance.theme,
      mode: options.uiAppearance.mode,
    });
  }
  if (options.workspace) {
    entries['neondojo.workspace'] = JSON.stringify({
      matchId: options.workspace.matchId,
      gameNo: options.workspace.gameNo ?? 1,
      ...(options.workspace.positionId
        ? { positionId: options.workspace.positionId }
        : {}),
    });
  }
  if (options.extra) {
    for (const [key, value] of Object.entries(options.extra)) {
      if (value !== null && value !== undefined) entries[key] = value;
    }
  }
  const locale = options.locale;
  const preserveLocale = options.preserveLocale ?? false;
  await page.addInitScript(
    ({ entries, locale, preserveLocale }) => {
      if (
        locale &&
        (!preserveLocale || !localStorage.getItem('neondojo.locale'))
      )
        localStorage.setItem('neondojo.locale', locale);
      for (const [key, value] of Object.entries(entries))
        localStorage.setItem(key, value);
    },
    { entries, locale, preserveLocale },
  );
}

/** Convenience wrapper that seeds `neondojo.locale` (default `'en'`) and optional extra keys. */
export function seedLocale(
  page: Page,
  locale: 'en' | 'de' = 'en',
  extra?: Omit<SeedStorageOptions, 'locale'>,
) {
  return seedStorage(page, { ...extra, locale });
}

export function workspaceTab(page: Page, workspace: WorkspaceTab): Locator {
  return page.getByRole('tab', { name: WORKSPACE_TAB_NAMES[workspace] });
}

export function workspacePanel(page: Page, workspace: WorkspaceTab): Locator {
  return page.getByRole('tabpanel', { name: WORKSPACE_TAB_NAMES[workspace] });
}

export async function openWorkspace(
  page: Page,
  workspace: WorkspaceTab,
): Promise<Locator> {
  await workspaceTab(page, workspace).click();
  return workspacePanel(page, workspace);
}

/** Navigates to `/` and clicks the target workspace tab. */
export async function gotoWorkspace(
  page: Page,
  workspace: WorkspaceTab,
): Promise<Locator> {
  await page.goto('/');
  return openWorkspace(page, workspace);
}

/** Opens the Positions workspace, selects a database (if given), and runs the search. */
export async function openPositionBrowser(
  page: Page,
  databaseId?: string,
): Promise<Locator> {
  const pane = await openWorkspace(page, 'positions');
  if (databaseId) {
    await pane
      .getByRole('combobox', { name: /^(Database|Datenbank)$/ })
      .selectOption(databaseId);
  }
  await pane
    .getByRole('button', { name: /^(Search positions|Positionen suchen)$/ })
    .click();
  return pane;
}

/** Navigates to `/` and waits for the Analysis workspace board to appear. */
export async function gotoAnalysisBoard(page: Page) {
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
}

import { expect, it, vi } from 'vitest';
import { preloadStartupAssets } from './startup-preload';

it('swallows archive-theme preload errors so the app mounts and LazyPanel can retry', async () => {
  const loadLocale = vi.fn().mockResolvedValue(undefined);
  const loadArchiveThemes = vi
    .fn()
    .mockRejectedValue(
      new TypeError('Failed to fetch dynamically imported module'),
    );

  await expect(
    preloadStartupAssets('en', {
      loadLocale,
      loadArchiveThemes,
      shouldPreloadArchiveThemes: () => true,
    }),
  ).resolves.toBeUndefined();

  expect(loadLocale).toHaveBeenCalledWith('en');
  expect(loadArchiveThemes).toHaveBeenCalledTimes(1);
});

it('skips archive-theme preloading when only built-in artwork is configured', async () => {
  const loadLocale = vi.fn().mockResolvedValue(undefined);
  const loadArchiveThemes = vi.fn().mockResolvedValue([]);

  await preloadStartupAssets('de', {
    loadLocale,
    loadArchiveThemes,
    shouldPreloadArchiveThemes: () => false,
  });

  expect(loadLocale).toHaveBeenCalledWith('de');
  expect(loadArchiveThemes).not.toHaveBeenCalled();
});

it('still rejects when the initial locale chunk cannot be loaded', async () => {
  const error = new Error('locale chunk failed');
  const loadLocale = vi.fn().mockRejectedValue(error);
  const loadArchiveThemes = vi.fn().mockResolvedValue([]);

  await expect(
    preloadStartupAssets('en', {
      loadLocale,
      loadArchiveThemes,
      shouldPreloadArchiveThemes: () => true,
    }),
  ).rejects.toThrow('locale chunk failed');
});

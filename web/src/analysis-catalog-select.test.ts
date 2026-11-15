import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { I18nProvider } from './i18n';
import AnalysisCatalogSelect from './AnalysisCatalogSelect';
import { catalogFixture } from './analysis-catalog-test-fixtures';
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
it('retains an unavailable explicit selection visibly instead of showing Automatic', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(AnalysisCatalogSelect, {
        catalog: { ...catalogFixture(), selectedId: 'job:removed' },
      }),
    }),
  );
  expect(html).toContain(
    '<option value="job:removed" selected="">Selected analysis unavailable</option>',
  );
  expect(html).toContain('Choose another saved analysis or Automatic.');
  expect(html).not.toContain('<option value="" selected="">');
});

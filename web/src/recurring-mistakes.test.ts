import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, test, vi } from 'vitest';
import { I18nProvider, type Locale } from './i18n';
import RecurringMistakes from './RecurringMistakes';
import { mistakeMessages } from './recurring-mistakes-messages';
afterEach(() => vi.unstubAllGlobals());
test.each(['en', 'de', 'es', 'fr', 'pl', 'it'] as Locale[])(
  'offers an explicit localized player choice before personal results in %s',
  (locale) => {
    vi.stubGlobal('localStorage', { getItem: () => locale });
    const html = renderToStaticMarkup(
      createElement(I18nProvider, {
        children: createElement(RecurringMistakes, {
          visible: true,
          theme: 'forest',
          onOpenPlayers: () => undefined,
          independentOnly: false,
          onIndependentOnly: () => undefined,
        }),
      }),
    );
    expect(html).toContain(mistakeMessages[locale].title);
    expect(html).toContain(mistakeMessages[locale].chooseHelp);
    expect(html).toContain(mistakeMessages[locale].coverage);
    expect(html).not.toContain('<table');
  },
);

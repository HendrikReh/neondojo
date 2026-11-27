import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import { I18nProvider } from '../../shared/i18n/i18n';
import AnalysisPanel from './AnalysisPanel';
import type { useJobs } from '../../shared/jobs/useJobs';
import type { PositionCalculation } from './usePositionCalculation';
import type { AnalysisCatalog } from './useAnalysisCatalog';

it('wires every AnalysisPanel tab to a matching role="tabpanel" container', () => {
  const noop = vi.fn();
  const positionJobs = {
    jobs: {},
    restoring: false,
    failure: undefined,
    submitting: undefined,
    error: undefined,
    run: noop,
    removeDice: noop,
    refresh: noop,
  } as unknown as ReturnType<typeof useJobs>;
  const calculation = {
    view: 'analysis',
    setView: noop,
    showDice: noop,
    pending: false,
    blocked: false,
    diceDeleted: false,
    settings: false,
  } as unknown as PositionCalculation;
  const catalog = {
    analysisId: undefined,
    selectedId: undefined,
    selected: undefined,
    sourceRevision: 0,
    entries: [],
    loading: false,
    saving: false,
    select: noop,
    refresh: noop,
  } as unknown as AnalysisCatalog;

  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(AnalysisPanel, {
        positionJobs,
        calculation,
        catalog,
        resultsVisible: true,
      }),
    }),
  );

  const tabs = [
    ...html.matchAll(
      /<button[^>]*role="tab"[^>]*id="([^"]+)"[^>]*aria-controls="([^"]+)"/g,
    ),
  ];
  expect(tabs).toHaveLength(4);
  for (const [, tabId, panelId] of tabs) {
    expect(html).toContain(
      `id="${panelId}" role="tabpanel" aria-labelledby="${tabId}"`,
    );
  }
});

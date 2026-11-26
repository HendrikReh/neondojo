import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { Position } from '../../shared/http/api';
import type { Locale } from '../../shared/i18n/i18n-core';
import { I18nProvider } from '../../shared/i18n/i18n';
import MoveHistoryRow from './MoveHistoryRow';
import type { MoveAssessment } from '../../shared/review/useMoveAssessments';

afterEach(() => vi.unstubAllGlobals());
const position: Position = {
  id: 'move-position',
  match_uuid: 'match',
  game_no: 3,
  ply: 12,
  ordinal: 12,
  player: 0,
  dice: [3, 1],
  board: Array(26).fill(0),
  notation: '24/21 6/5',
  position_id: '',
  gnubg_match_id: '',
};
const assessment: MoveAssessment = {
  position_id: position.id,
  source: 'native',
  severity: 'bad',
  equity_loss: 0.092,
  engine: 'GNUbg fixture',
  depth: '2-ply',
  depth_plies: 2,
  job_id: null,
};
function render(
  locale: Locale = 'en',
  value: MoveAssessment | undefined = assessment,
  player = 0,
) {
  vi.stubGlobal('localStorage', { getItem: () => locale });
  return renderToStaticMarkup(
    createElement(
      I18nProvider,
      null,
      createElement(MoveHistoryRow, {
        position: { ...position, player },
        assessment: value,
        players: ['Mochy', 'Hendrik'],
        current: true,
        onSelect: () => {},
      }),
    ),
  );
}

it('shows notation, named player and dice with a numeric loss instead of a severity glyph', () => {
  const html = render();
  expect(html).toContain('move-history-context');
  expect(html).toContain('24/21 6/5');
  expect(html).toContain('Mochy');
  expect(html).toContain('3–1');
  expect(html).toContain('0.092');
  expect(html).toContain('GNUbg fixture · 2-ply');
  expect(html).toContain('data-position-id="move-position"');
  expect(html).toContain('aria-current="step"');
  expect(html).toContain('3.12');
  expect(render('en', assessment, 1)).toContain('Hendrik');
});

it('keeps source and severity identity for every grade including a genuine zero', () => {
  for (const [severity, loss] of [
    ['none', 0],
    ['doubtful', 0.045],
    ['bad', 0.092],
    ['very_bad', 0.188],
  ] as const) {
    const html = render('en', {
      ...assessment,
      severity,
      equity_loss: loss,
      source: 'imported',
    });
    expect(html).toContain(`move-assessment-${severity}`);
    expect(html).toContain('data-assessment-source="imported"');
    expect(html).toContain(loss.toFixed(3));
  }
});

it('leaves missing analysis neutral rather than inventing zero loss', () => {
  vi.stubGlobal('localStorage', { getItem: () => 'en' });
  const html = renderToStaticMarkup(
    createElement(
      I18nProvider,
      null,
      createElement(MoveHistoryRow, {
        position,
        players: ['Mochy', 'Hendrik'],
        current: false,
        onSelect: () => {},
      }),
    ),
  );
  expect(html).toContain('move-assessment-unavailable');
  expect(html).toContain('Not evaluated');
  expect(html).not.toContain('move-assessment-marker');
  expect(html).not.toContain('0.000');
  expect(html).not.toContain('aria-current');
});

it('formats loss in both interface languages', () => {
  for (const locale of ['en', 'de'] as const) {
    const html = render(locale);
    expect(html).toContain(locale === 'en' ? '0.092' : '0,092');
    expect(html).not.toContain('undefined');
  }
});

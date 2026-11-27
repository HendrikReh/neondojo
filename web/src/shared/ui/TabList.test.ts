import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { nextRovingTab, TabList, type TabItem } from './TabList';

it('wraps horizontally and handles Home and End', () => {
  const tabs = ['analysis', 'dice', 'observed'] as const;
  expect(nextRovingTab(tabs, 'analysis', 'ArrowRight')).toBe('dice');
  expect(nextRovingTab(tabs, 'observed', 'ArrowRight')).toBe('analysis');
  expect(nextRovingTab(tabs, 'analysis', 'ArrowLeft')).toBe('observed');
  expect(nextRovingTab(tabs, 'dice', 'Home')).toBe('analysis');
  expect(nextRovingTab(tabs, 'dice', 'End')).toBe('observed');
  expect(nextRovingTab(tabs, 'dice', 'ArrowDown')).toBeUndefined();
  expect(nextRovingTab(tabs, 'dice', 'Enter')).toBeUndefined();
});

it('supports vertical clamping without wrapping', () => {
  const sections = ['general', 'appearance', 'profiles'] as const;
  const options = { orientation: 'vertical' as const, wrap: false };
  expect(nextRovingTab(sections, 'general', 'ArrowUp', options)).toBe(
    'general',
  );
  expect(nextRovingTab(sections, 'general', 'ArrowDown', options)).toBe(
    'appearance',
  );
  expect(nextRovingTab(sections, 'profiles', 'ArrowDown', options)).toBe(
    'profiles',
  );
  expect(nextRovingTab(sections, 'appearance', 'Home', options)).toBe(
    'general',
  );
  expect(nextRovingTab(sections, 'appearance', 'End', options)).toBe(
    'profiles',
  );
  expect(nextRovingTab(sections, 'appearance', 'ArrowRight', options)).toBe(
    undefined,
  );
});

it('renders accessible tablist markup with roving tabIndex and fallback focus stop', () => {
  const tabs: TabItem<'analysis' | 'comparison' | 'dice'>[] = [
    {
      value: 'analysis',
      label: 'Candidates',
      id: 'tab-analysis',
      controls: 'panel-analysis',
    },
    {
      value: 'comparison',
      label: 'Compare',
      disabled: true,
    },
    {
      value: 'dice',
      label: 'Dice',
      id: 'tab-dice',
      controls: 'panel-dice',
    },
  ];
  const html = renderToStaticMarkup(
    createElement(TabList<'analysis' | 'comparison' | 'dice'>, {
      as: 'nav',
      className: 'tabs',
      ariaLabel: 'Views',
      tabs,
      active: 'dice',
      onSelect: () => {},
    }),
  );
  expect(html).toContain('<nav class="tabs" role="tablist" aria-label="Views">');
  expect(html).toContain(
    'id="tab-analysis" aria-controls="panel-analysis" aria-selected="false" tabindex="-1"',
  );
  expect(html).toContain('disabled=""');
  expect(html).toContain(
    'id="tab-dice" aria-controls="panel-dice" aria-selected="true" tabindex="0"',
  );

  // When active is null (e.g. Home view), the first enabled tab keeps tabIndex=0.
  const homeHtml = renderToStaticMarkup(
    createElement(TabList<'analysis' | 'comparison' | 'dice'>, {
      ariaLabel: 'Views',
      tabs,
      active: null,
      onSelect: () => {},
    }),
  );
  expect(homeHtml).toContain(
    'id="tab-analysis" aria-controls="panel-analysis" aria-selected="false" tabindex="0"',
  );
  expect(homeHtml).toContain(
    'id="tab-dice" aria-controls="panel-dice" aria-selected="false" tabindex="-1"',
  );
});

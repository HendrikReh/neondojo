import { describe, expect, it, vi, afterEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { useResizableColumns } from './useResizableColumns';
import { createColumnLayout } from './column-widths';
import { useLibraryColumns } from './useLibraryColumns';
import {
  defaultConfiguration,
  type ProfileConfiguration,
} from './configuration-model';

const profile = vi.hoisted(() => ({ value: null as unknown }));
vi.mock('./ConfigurationProvider', () => ({
  useOptionalConfiguration: () => profile.value,
}));
afterEach(() => {
  profile.value = null;
});

const layout = createColumnLayout(
  [
    { id: 'first', className: 'first', min: 40 },
    { id: 'second', className: 'second', min: 50 },
  ] as const,
  'test.profile.columns',
);

describe('profile-controlled table widths', () => {
  it('renders the active configuration rather than a stale browser preference', () => {
    function Table() {
      const options = {
        visible: true,
        resizeLabel: (label: string) => label,
        widths: { first: 151, second: 223 },
        onWidthsChange: () => {},
        configurationGeneration: 4,
      };
      const columns = useResizableColumns(layout, options);
      return createElement(
        'table',
        { style: columns.tableStyle },
        createElement(
          'tbody',
          {},
          createElement(
            'tr',
            {},
            createElement('td', {}, columns.handle('first', 'First')),
          ),
        ),
      );
    }
    const html = renderToStaticMarkup(createElement(Table));
    expect(html).toContain('width:374px');
    expect(html).toContain('aria-valuenow="151"');
  });
  it('keeps automatic layout automatic without writing measured widths', () => {
    let writes = 0;
    function Table() {
      const options = {
        visible: true,
        resizeLabel: (label: string) => label,
        widths: null,
        onWidthsChange: () => {
          writes++;
        },
        configurationGeneration: 5,
      };
      const columns = useResizableColumns(layout, options);
      return createElement(
        'table',
        { style: columns.tableStyle },
        createElement(
          'tbody',
          {},
          createElement(
            'tr',
            {},
            createElement('td', {}, String(columns.custom)),
          ),
        ),
      );
    }
    expect(renderToStaticMarkup(createElement(Table))).not.toContain('width:');
    expect(writes).toBe(0);
  });
  it('restores visibility, column order and explicit widths across profile round trips', () => {
    const first = structuredClone(defaultConfiguration);
    first.library.visible_columns = ['id', 'white'];
    first.library.column_widths = {
      id: 125,
      white: 201,
      occurrence_database: 240,
      occurrence_game: 90,
      occurrence_move: 100,
    };
    const second = structuredClone(defaultConfiguration);
    second.library.visible_columns = ['black', 'id'];
    second.library.column_order = [
      'black',
      ...second.library.column_order.filter((id) => id !== 'black'),
    ];
    second.library.column_widths = {
      black: 255,
      id: 99,
      occurrence_database: 300,
      occurrence_game: 110,
      occurrence_move: 120,
    };
    const state = {
      current: first,
      generation: 1,
      updateCurrent: (
        update: (c: ProfileConfiguration) => ProfileConfiguration,
      ) => {
        state.current = update(state.current);
      },
    };
    profile.value = state;
    let controls: ReturnType<typeof useLibraryColumns>;
    function Table() {
      controls = useLibraryColumns(false, true);
      return createElement(
        'p',
        {},
        JSON.stringify({
          columns: controls.activeColumns,
          widths: controls.widths,
        }),
      );
    }
    const firstHtml = renderToStaticMarkup(createElement(Table));
    expect(firstHtml).toContain('select');
    expect(controls!.activeColumns).toEqual([
      'select',
      'id',
      'white',
      'actions',
    ]);
    expect(controls!.widths?.id).toBe(125);
    expect(controls!.widths?.occurrence_database).toBe(240);
    expect(controls!.widths?.occurrence_game).toBe(90);
    expect(controls!.widths?.occurrence_move).toBe(100);
    state.current = second;
    state.generation++;
    renderToStaticMarkup(createElement(Table));
    expect(controls!.activeColumns).toEqual([
      'select',
      'black',
      'id',
      'actions',
    ]);
    expect(controls!.widths?.black).toBe(255);
    expect(controls!.widths?.occurrence_database).toBe(300);
    expect(controls!.widths?.occurrence_game).toBe(110);
    expect(controls!.widths?.occurrence_move).toBe(120);
    state.current = first;
    state.generation++;
    expect(renderToStaticMarkup(createElement(Table))).toBe(firstHtml);
    controls!.toggle('black', true);
    expect(state.current.library.visible_columns).toContain('black');
    controls!.move('white', -1);
    expect(state.current.library.column_order.indexOf('white')).toBe(0);
    expect(first.library.visible_columns).toEqual(['id', 'white']);
  });
});

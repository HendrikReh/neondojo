import type { AnalysisCatalogEntry } from './analysis-catalog-api';
import type { AnalysisCatalog } from './useAnalysisCatalog';
import { useAnalysisCatalogText } from './analysis-catalog-messages';
import { useI18n } from './i18n';
import { shortEngineLabel } from './engine-version';

export function useAnalysisEntryLabel() {
  const text = useAnalysisCatalogText();
  const { locale } = useI18n();
  const date = new Intl.DateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
  return (entry: AnalysisCatalogEntry) =>
    [
      entry.job ? shortEngineLabel(entry.engine) : entry.engine,
      entry.depth,
      entry.scope === 'game'
        ? text('game', { number: entry.game_no ?? '—' })
        : text(entry.scope),
      entry.status === 'running' ||
      entry.status === 'queued' ||
      entry.status === 'failed'
        ? text(entry.status)
        : entry.job && !entry.complete && text('partial'),
      date.format(new Date(entry.created_at)),
    ]
      .filter(Boolean)
      .join(' · ');
}

export default function AnalysisCatalogSelect({
  catalog,
}: {
  catalog: AnalysisCatalog;
}) {
  const text = useAnalysisCatalogText();
  const label = useAnalysisEntryLabel();
  const { formatError, formatNumber } = useI18n();
  const selected = catalog.selected;
  const entries =
    catalog.selected &&
    !catalog.entries.some((entry) => entry.id === catalog.selected?.id)
      ? [catalog.selected, ...catalog.entries]
      : catalog.entries;
  const unavailable =
    catalog.selectedId &&
    !entries.some((entry) => entry.id === catalog.selectedId);
  return (
    <div className="analysis-catalog-control">
      <label>
        <span>{text('analysis')}</span>
        <select
          aria-label={text('analysis')}
          title={
            selected &&
            [
              selected.engine,
              text(selected.source),
              text('coverage', {
                checker: formatNumber(selected.checker_count),
                cube: formatNumber(selected.cube_count),
              }),
            ]
              .filter(Boolean)
              .join(' · ')
          }
          value={catalog.selectedId ?? ''}
          disabled={catalog.loading || catalog.saving}
          onChange={(event) => void catalog.select(event.target.value || null)}
        >
          <option value="">
            {text('automatic')}
            {catalog.selected ? ` · ${label(catalog.selected)}` : ''}
          </option>
          {unavailable && (
            <option value={catalog.selectedId!}>{text('unavailable')}</option>
          )}
          {entries.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {label(entry)}
            </option>
          ))}
        </select>
      </label>
      {unavailable && <p role="status">{text('chooseAnother')}</p>}
      {(catalog.loading || catalog.saving) && (
        <span role="status">{text(catalog.saving ? 'saving' : 'loading')}</span>
      )}
      {catalog.nextCursor && (
        <button
          type="button"
          disabled={catalog.loadingMore || catalog.saving}
          onClick={() => void catalog.loadMore()}
        >
          {text('more')}
        </button>
      )}
      {!!catalog.error && (
        <>
          <p className="error" role="alert">
            {formatError(catalog.error)}
          </p>
          <button type="button" onClick={catalog.refresh}>
            {text('retry')}
          </button>
        </>
      )}
    </div>
  );
}

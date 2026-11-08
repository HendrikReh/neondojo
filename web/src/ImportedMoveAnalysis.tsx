import { useEffect, useState } from 'react';
import { request } from './api';
import type { SourceAnalysis } from './source-analysis-api';
import SourceAnalysisView from './SourceAnalysisView';
import { useSourceText } from './analysis-source-messages';
import { useI18n } from './i18n';
export default function ImportedMoveAnalysis({ moveId }: { moveId: string }) {
  const l = useSourceText(),
    { formatError } = useI18n();
  const [state, setState] = useState<{
    id: string;
    records: SourceAnalysis[];
    error?: unknown;
  }>({ id: moveId, records: [] });
  useEffect(() => {
    const abort = new AbortController();
    setState({ id: moveId, records: [] });
    void request<SourceAnalysis[]>(`/moves/${moveId}/imported-analysis`, {
      signal: abort.signal,
    })
      .then((records) => {
        if (!abort.signal.aborted) setState({ id: moveId, records });
      })
      .catch((error) => {
        if (!abort.signal.aborted) setState({ id: moveId, records: [], error });
      });
    return () => abort.abort();
  }, [moveId]);
  if (state.id !== moveId) return null;
  if (state.error)
    return (
      <p className="error" role="alert">
        {formatError(state.error)}
      </p>
    );
  return state.records.length ? (
    <section className="imported-move-analysis">
      <h3>{l('imported')}</h3>
      <SourceAnalysisView records={state.records} source="file" />
    </section>
  ) : null;
}

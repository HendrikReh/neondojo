import { useEffect, useMemo, useState } from 'react';
import { request, type Match } from '../../../shared/http/api';
import type { SourceAnalysis } from './source-analysis-api';
import SourceAnalysisView from './SourceAnalysisView';
import { useSourceText } from './analysis-source-messages';
import { useI18n } from '../../../shared/i18n/i18n';
export default function ImportedMoveAnalysis({
  moveId,
  players,
  decisionKind,
}: {
  moveId: string;
  players: Pick<Match, 'white' | 'black'>;
  decisionKind?: 'checker' | 'cube';
}) {
  const l = useSourceText(),
    { formatError } = useI18n();
  const [state, setState] = useState<{
    id: string;
    records: SourceAnalysis[];
    error?: unknown;
  }>({ id: moveId, records: [] });
  const records = useMemo(
    () =>
      decisionKind
        ? state.records.filter((record) => record.kind === decisionKind)
        : state.records,
    [state.records, decisionKind],
  );
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
  return records.length ? (
    <section className="imported-move-analysis">
      <h3>{l('imported')}</h3>
      <SourceAnalysisView records={records} source="file" players={players} />
    </section>
  ) : null;
}

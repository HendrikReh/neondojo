import { useEffect, useState } from 'react';
import { request } from '../../../shared/http/api';
import {
  analysisStatisticsPath,
  type MatchAnalysisStatistics,
} from '../analysis-catalog-api';

export function useMatchStatistics(
  matchId: string | undefined,
  analysisId: string | undefined,
  statisticsKey: string,
) {
  const [statisticsResult, setStatisticsResult] = useState<{
    key: string;
    value: MatchAnalysisStatistics;
  }>();
  const [statisticsError, setStatisticsError] = useState<{
    key: string;
    error: unknown;
  }>();
  const [statisticsLoaded, setStatisticsLoaded] = useState<string>();

  useEffect(() => {
    if (!matchId || !analysisId) return;
    const abort = new AbortController();
    void request<MatchAnalysisStatistics>(
      analysisStatisticsPath(matchId, analysisId),
      { signal: abort.signal },
    )
      .then((value) => {
        if (abort.signal.aborted || value.analysis_id !== analysisId) return;
        setStatisticsResult({ key: statisticsKey, value });
        setStatisticsError(undefined);
      })
      .catch((error) => {
        if (!abort.signal.aborted)
          setStatisticsError({ key: statisticsKey, error });
      })
      .finally(() => {
        if (!abort.signal.aborted) setStatisticsLoaded(statisticsKey);
      });
    return () => abort.abort();
  }, [matchId, analysisId, statisticsKey]);

  return {
    statistics:
      statisticsResult?.key === statisticsKey
        ? statisticsResult.value
        : undefined,
    statisticsLoading: !!analysisId && statisticsLoaded !== statisticsKey,
    statisticsError:
      statisticsError?.key === statisticsKey
        ? statisticsError.error
        : undefined,
  };
}

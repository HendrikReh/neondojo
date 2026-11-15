import { useEffect, useState } from 'react';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';
import { useI18n } from './i18n';
import { loadEngines, type EngineInfo } from './settings-api';
import { useSettingsText } from './settings-messages';
import type { MatchAnalysisEngine } from './useMatchAnalysis';

export function useMatchAnalysisOptions({
  defaultEngine,
  defaultPlies,
  configured,
  analysisInstallationKey,
}: {
  defaultEngine: MatchAnalysisEngine;
  defaultPlies: number;
  configured: boolean;
  analysisInstallationKey?: string;
}) {
  const [engineId, setEngineId] = useState(defaultEngine);
  const [requestedPlies, setPlies] = useState(defaultPlies);
  useEffect(() => {
    setEngineId(defaultEngine);
    setPlies(defaultPlies);
  }, [defaultEngine, defaultPlies]);
  const [engineCatalog, setEngineCatalog] = useState<{
    key?: string;
    engines: EngineInfo[];
    error?: unknown;
  }>();
  useEffect(() => {
    if (!configured) return;
    const abort = new AbortController();
    void loadEngines(abort.signal)
      .then((engines) => {
        if (!abort.signal.aborted)
          setEngineCatalog({ key: analysisInstallationKey, engines });
      })
      .catch((error) => {
        if (!abort.signal.aborted)
          setEngineCatalog({
            key: analysisInstallationKey,
            engines: [],
            error,
          });
      });
    return () => abort.abort();
  }, [configured, analysisInstallationKey]);
  const catalog =
    engineCatalog?.key === analysisInstallationKey ? engineCatalog : undefined;
  const engine = catalog?.engines.find((item) => item.id === engineId);
  const supportedPlies = configurationOptions.engine_profiles[
    engineId
  ].analysis.filter((depth) => !engine || engine.plies.includes(depth));
  const plies = supportedPlies.includes(requestedPlies)
    ? requestedPlies
    : supportedPlies.includes(DEFAULT_ANALYSIS_PLIES)
      ? DEFAULT_ANALYSIS_PLIES
      : (supportedPlies[0] ?? requestedPlies);
  return {
    engineId,
    plies,
    supportedPlies,
    catalog,
    analysisConfigured:
      configured && !!engine?.available && supportedPlies.includes(plies),
    onEngine: (value: MatchAnalysisEngine) => {
      setEngineId(value);
      setPlies(plies);
    },
    onPlies: setPlies,
  };
}

export function MatchAnalysisFields({
  options,
  disabled,
}: {
  options: ReturnType<typeof useMatchAnalysisOptions>;
  disabled: boolean;
}) {
  const { t } = useI18n();
  const settingsText = useSettingsText();
  const {
    engineId,
    plies,
    supportedPlies,
    catalog,
    analysisConfigured,
    onEngine,
    onPlies,
  } = options;
  return (
    <>
      <label>
        {settingsText('analysisEngine')}
        <select
          aria-label={settingsText('analysisEngine')}
          value={engineId}
          disabled={disabled}
          onChange={(event) => {
            const value = event.target.value;
            if (value !== 'gnubg' && value !== 'bgsage') return;
            onEngine(value);
          }}
        >
          {(['gnubg', 'bgsage'] as const).map((id) => {
            const item = catalog?.engines.find((entry) => entry.id === id);
            const name = id === 'gnubg' ? 'GNUbg' : 'Open Sage';
            return (
              <option key={id} value={id}>
                {item?.name ?? name}
                {catalog && !item?.available
                  ? ` — ${settingsText('unavailable')}`
                  : ''}
              </option>
            );
          })}
        </select>
      </label>
      <label>
        {t('matchAnalysis.depth')}{' '}
        <select
          aria-label={t('matchAnalysis.depthLabel')}
          value={plies}
          disabled={disabled || !analysisConfigured}
          onChange={(event) => onPlies(Number(event.target.value))}
        >
          <option value={0} disabled={!supportedPlies.includes(0)}>
            {t('matchAnalysis.fast')}
          </option>
          <option value={1} disabled={!supportedPlies.includes(1)}>
            1-Ply
          </option>
          <option value={2} disabled={!supportedPlies.includes(2)}>
            {t('matchAnalysis.thorough')}
          </option>
          <option value={3} disabled={!supportedPlies.includes(3)}>
            3-ply
          </option>
          <option value={4} disabled={!supportedPlies.includes(4)}>
            4-ply
          </option>
        </select>
      </label>
    </>
  );
}

import configurationOptions from '../../../../shared/configuration-options.json' with { type: 'json' };
import { useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { useRovingTabs } from '../../shared/ui/TabList';
import {
  ANALYSIS_PLIES,
  PLAY_PLIES,
  preferredDepth as enginePreferredDepth,
} from '../../shared/engine/analysis-depth';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import type { EngineInfo } from '../../settings/settings-api';
import type { BenchmarkReference } from './benchmark-api';
import { useBenchmarkText } from './benchmark-messages';

function preferredDepth(engineId: string, depths: readonly number[]) {
  const preferred = enginePreferredDepth(engineId);
  return depths.includes(preferred) ? preferred : depths[0];
}

export default function BenchmarkSetupForm({
  mode,
  onModeChange,
  engines,
  engine,
  engineId,
  onEngineChange,
  engineError,
  reference,
  referenceError,
  defaultPlies,
  playPlies,
  canStart,
  starting,
  onStartAnalysis,
  onStartPlay,
}: {
  mode: 'analysis' | 'play';
  onModeChange: (mode: 'analysis' | 'play') => void;
  engines: EngineInfo[];
  engine: EngineInfo | undefined;
  engineId: string;
  onEngineChange: (engineId: string) => void;
  engineError: unknown;
  reference: BenchmarkReference | undefined;
  referenceError: unknown;
  defaultPlies: number;
  playPlies: number | undefined;
  canStart: boolean;
  starting: 'analysis' | 'play' | undefined;
  onStartAnalysis: (plies: number[]) => void;
  onStartPlay: (depth: number | undefined) => void;
}) {
  const t = useBenchmarkText();
  const { formatNumber } = useI18n();
  const [playDepthsByEngine, setPlayDepthsByEngine] = useState<
    Record<string, number>
  >({});
  const [analysisDepthsByEngine, setAnalysisDepthsByEngine] = useState<
    Record<string, number[]>
  >({});

  const requestedAnalysisDepths = analysisDepthsByEngine[engineId] ?? [
    defaultPlies,
  ];
  const availableAnalysisDepths = engine?.plies ?? [];
  const compatiblePlies = requestedAnalysisDepths
    .filter((ply) => availableAnalysisDepths.includes(ply))
    .sort((a, b) => a - b);
  const analysisFallback =
    requestedAnalysisDepths.length > 0 && compatiblePlies.length === 0
      ? preferredDepth(engineId, availableAnalysisDepths)
      : undefined;
  const selected =
    analysisFallback === undefined ? compatiblePlies : [analysisFallback];

  const supportedPlayDepths = (
    engineId === 'bgsage'
      ? configurationOptions.engine_profiles.bgsage.play
      : configurationOptions.engine_profiles.gnubg.play
  ).filter((ply) => engine?.plies.includes(ply));
  const requestedPlayDepth =
    playDepthsByEngine[engineId] ?? playPlies ?? enginePreferredDepth(engineId);
  const playDepthSupported = supportedPlayDepths.includes(requestedPlayDepth);
  const playFallback = playDepthSupported
    ? undefined
    : preferredDepth(engineId, supportedPlayDepths);
  const selectedPlayDepth = playDepthSupported
    ? requestedPlayDepth
    : playFallback;

  const play = mode === 'play';
  const modeNav = useRovingTabs<'analysis' | 'play'>({
    items: ['analysis', 'play'],
    active: mode,
    onSelect: onModeChange,
  });
  const canStartAnalysis =
    canStart &&
    referenceError === undefined &&
    !!reference &&
    !!engine &&
    selected.length > 0;
  const canStartPlay = canStart && selectedPlayDepth !== undefined;

  return (
    <>
      <div className="benchmark-engine-row">
        <label className="benchmark-engine">
          {t('engine')}
          <select
            aria-label={t('engine')}
            value={engineId}
            disabled={Boolean(starting)}
            onChange={(event) => onEngineChange(event.target.value)}
          >
            {engines.length === 0 && engineError === undefined && (
              <option value={engineId}>{t('loading')}</option>
            )}
            {engines.map((e) => (
              <option key={e.id} value={e.id} disabled={!e.available}>
                {e.name}
              </option>
            ))}
          </select>
        </label>
        {!engine && engines.length > 0 && <p role="status">{t('noEngine')}</p>}
      </div>
      <div
        className="benchmark-modes text-navigation text-navigation--compact"
        role="group"
        aria-label={t('mode')}
      >
        <button
          ref={modeNav.getTabRef('analysis')}
          type="button"
          aria-pressed={!play}
          onClick={() => onModeChange('analysis')}
          onKeyDown={(event) => modeNav.onKeyDown(event, 'analysis')}
        >
          {t('matchAnalysis')}
        </button>
        <button
          ref={modeNav.getTabRef('play')}
          type="button"
          aria-pressed={play}
          onClick={() => onModeChange('play')}
          onKeyDown={(event) => modeNav.onKeyDown(event, 'play')}
        >
          {t('playLatency')}
        </button>
      </div>
      {play ? (
        <form
          className="benchmark-configuration"
          onSubmit={(event) => {
            event.preventDefault();
            onStartPlay(selectedPlayDepth);
          }}
        >
          <div className="benchmark-controls">
            <label>
              {t('playPly')}
              <select
                value={selectedPlayDepth ?? ''}
                disabled={Boolean(starting)}
                onChange={(event) => {
                  const value = Number(event.target.value);
                  setPlayDepthsByEngine((current) => ({
                    ...current,
                    [engineId]: value,
                  }));
                }}
              >
                {PLAY_PLIES.map((ply) => (
                  <option
                    key={ply}
                    value={ply}
                    disabled={!supportedPlayDepths.includes(ply)}
                  >
                    {formatPly(ply, formatNumber)}
                  </option>
                ))}
              </select>
            </label>
            <ActionButton
              icon="play"
              className="primary"
              type="submit"
              label={t(starting === 'play' ? 'starting' : 'startPlay')}
              disabled={!canStartPlay}
            />
          </div>
          {playFallback !== undefined && (
            <p className="benchmark-help" role="status">
              {t('depthFallback', { ply: formatNumber(playFallback) })}
            </p>
          )}
          <p className="benchmark-help">{t('playTiming')}</p>
          {playPlies !== undefined && (
            <p className="benchmark-help">
              {t('playDepth', { ply: formatNumber(playPlies) })}
            </p>
          )}
        </form>
      ) : (
        <form
          className="benchmark-configuration"
          onSubmit={(event) => {
            event.preventDefault();
            onStartAnalysis(selected);
          }}
        >
          <div className="benchmark-controls">
            <fieldset>
              <legend>{t('plies')}</legend>
              <div className="benchmark-plies">
                {ANALYSIS_PLIES.map((ply) => (
                  <label key={ply}>
                    <input
                      type="checkbox"
                      checked={selected.includes(ply)}
                      disabled={
                        !engine?.plies.includes(ply) || Boolean(starting)
                      }
                      onChange={(event) => {
                        const checked = event.target.checked;
                        setAnalysisDepthsByEngine((current) => ({
                          ...current,
                          [engineId]: checked
                            ? [...selected, ply]
                            : selected.filter((value) => value !== ply),
                        }));
                      }}
                    />
                    {formatPly(ply, formatNumber)}
                  </label>
                ))}
              </div>
            </fieldset>
            <ActionButton
              icon="play"
              className="primary"
              type="submit"
              label={t(starting === 'analysis' ? 'starting' : 'start')}
              disabled={!canStartAnalysis}
            />
          </div>
          {analysisFallback !== undefined && (
            <p className="benchmark-help" role="status">
              {t('depthFallback', { ply: formatNumber(analysisFallback) })}
            </p>
          )}
          <p className="benchmark-help">{t('serial')}</p>
        </form>
      )}
    </>
  );
}

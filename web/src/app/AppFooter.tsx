import { useEffect, useState } from 'react';
import type { components } from '../generated-api';
import { loadEngines, type AppSettings } from '../settings/settings-api';
import { engineDisplayName } from '../shared/engine/engine-version';
import { request } from '../shared/http/api';
import { useI18n } from '../shared/i18n/i18n';

const healthLabel = {
  checking: 'app.healthChecking',
  ready: 'app.engineConnected',
  unavailable: 'app.healthUnavailable',
  failed: 'app.healthCheckFailed',
} as const;

export default function AppFooter({
  engine,
  settingsOpen,
  settingsVersion,
}: {
  engine: AppSettings['analysis_engine'];
  settingsOpen: boolean;
  settingsVersion?: number;
}) {
  const { t } = useI18n();
  const [health, setHealth] = useState<keyof typeof healthLabel>('checking');
  useEffect(() => {
    if (settingsOpen) return;
    const abort = new AbortController();
    setHealth('checking');
    // The lightweight health endpoint probes GNUbg. The catalog verifies NeonGammon.
    const check =
      engine === 'gnubg'
        ? request<components['schemas']['Health']>('/health', {
            signal: abort.signal,
          }).then((value) => value.status === 'ok')
        : loadEngines(abort.signal).then((catalog) =>
            catalog.some((entry) => entry.id === engine && entry.available),
          );
    void check.then(
      (ready) => {
        if (!abort.signal.aborted) setHealth(ready ? 'ready' : 'unavailable');
      },
      () => {
        if (!abort.signal.aborted) setHealth('failed');
      },
    );
    return () => abort.abort();
  }, [engine, settingsOpen, settingsVersion]);

  return (
    <footer className="app-footer">
      <span>
        <span className={`status-dot ${health === 'ready' ? 'ready' : ''}`} />
        {t(healthLabel[health], { engine: engineDisplayName(engine) })}
      </span>
      <a href="/api/openapi.json" target="_blank" rel="noreferrer">
        OpenAPI ↗
      </a>
    </footer>
  );
}

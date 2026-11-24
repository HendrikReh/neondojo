import { useEffect, useRef, useState } from 'react';
import { request } from '../shared/http/api';
import { loadSettings, type AppSettings } from '../settings/settings-api';

/** The settings of this installation, loaded at the start and on request. */
export function useInstalledSettings() {
  const [settings, setSettings] = useState<AppSettings>();
  const [error, setError] = useState<unknown>();
  const [revision, setRevision] = useState(0);
  const loading = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    const abort = new AbortController();
    loading.current = abort;
    void loadSettings(abort.signal)
      .then((value) => {
        if (abort.signal.aborted) return;
        setSettings((current) =>
          current && current.version > value.version ? current : value,
        );
        setError(undefined);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    return () => abort.abort();
  }, [revision]);
  return {
    settings,
    error,
    revision,
    reload: () => setRevision((value) => value + 1),
    /** Shows saved settings; a running load would be older. */
    apply: (saved: AppSettings) => {
      loading.current?.abort();
      setSettings(saved);
      setError(undefined);
    },
  };
}

/** Whether the backend and its engine respond, checked per settings version. */
export function useHealth(settingsVersion: number | undefined) {
  const [health, setHealth] = useState<'checking' | 'ok' | 'degraded'>(
    'checking',
  );
  useEffect(() => {
    const abort = new AbortController();
    void request<{ status: string }>('/health', { signal: abort.signal })
      .then((value) => setHealth(value.status === 'ok' ? 'ok' : 'degraded'))
      .catch(() => {
        if (!abort.signal.aborted) setHealth('degraded');
      });
    return () => abort.abort();
  }, [settingsVersion]);
  return health;
}

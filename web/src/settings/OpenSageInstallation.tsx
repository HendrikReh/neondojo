import { useEffect, useId, useRef, useState } from 'react';
import {
  loadSageInstallation,
  saveSageInstallation,
  type SageInstallation,
  type EngineInfo,
} from './settings-api';
import ActionButton from '../shared/ui/ActionButton';
import EngineLabel from '../shared/engine/EngineLabel';
import { depthLabel } from '../shared/engine/analysis-depth';
import { isConflict } from '../shared/http/api';
import { useI18n } from '../shared/i18n/i18n';
import { useSettingsText } from './settings-messages';

export function useOpenSageInstallation(
  open: boolean,
  settingsReload: number,
  refreshEngines: () => void,
) {
  const [reload, setReload] = useState(0);
  const [installation, setInstallation] = useState<SageInstallation>();
  const [path, setPath] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const saveAbort = useRef<AbortController | null>(null);
  useEffect(() => {
    if (!open) return;
    const abort = new AbortController();
    setInstallation(undefined);
    setPath('');
    setBusy(false);
    setError(undefined);
    void loadSageInstallation(abort.signal)
      .then((value) => {
        if (!abort.signal.aborted) {
          setInstallation(value);
          setPath(value.python_path);
        }
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    return () => {
      abort.abort();
      saveAbort.current?.abort();
      saveAbort.current = null;
    };
  }, [open, reload, settingsReload]);
  async function verify() {
    if (!installation || saveAbort.current) return;
    const abort = new AbortController();
    saveAbort.current = abort;
    setBusy(true);
    setError(undefined);
    try {
      const value = await saveSageInstallation(
        installation.version,
        path,
        abort.signal,
      );
      if (!abort.signal.aborted) {
        setInstallation(value);
        setPath(value.python_path);
        refreshEngines();
      }
    } catch (e) {
      if (!abort.signal.aborted) {
        setError(e);
        if (path === installation.python_path) refreshEngines();
        throw e;
      }
    } finally {
      if (saveAbort.current === abort) {
        saveAbort.current = null;
        setBusy(false);
      }
    }
  }
  return {
    installation,
    path,
    setPath,
    busy,
    error,
    dirty: !!installation && path !== installation.python_path,
    verify,
    reload: () => {
      setReload((value) => value + 1);
      refreshEngines();
    },
    discard: () => {
      setPath(installation?.python_path ?? '');
      setError(undefined);
    },
  };
}

export default function OpenSageInstallation({
  setup,
  engine,
}: {
  setup: ReturnType<typeof useOpenSageInstallation>;
  engine?: EngineInfo;
}) {
  const { locale, formatError } = useI18n();
  const t = useSettingsText();
  const saveTargetId = useId();
  const { installation, path, setPath, busy, error, verify, reload } = setup;
  const snapshot =
    path === installation?.python_path ? installation.snapshot : null;
  const formatDepths = (plies: number[]) =>
    new Intl.ListFormat(locale, { type: 'unit' }).format(
      plies.map((ply) => depthLabel(ply)),
    );
  return (
    <fieldset className="settings-section settings-engine" disabled={busy}>
      <legend>{t('planned')}</legend>
      <div className="settings-engine-path">
        <label className="settings-field">
          <span>{t('sagePythonPath')}</span>
          <input
            value={path}
            title={path}
            disabled={!installation}
            onChange={(e) => setPath(e.target.value)}
            spellCheck={false}
            autoComplete="off"
          />
        </label>
        <ActionButton
          icon="confirm"
          label={t(busy ? 'sageVerifying' : 'sageVerify')}
          type="button"
          aria-describedby={saveTargetId}
          disabled={!installation || !path.trim()}
          onClick={() => void verify().catch(() => {})}
        />
      </div>
      <div className="settings-engine-status" role="status">
        {snapshot ? (
          <>
            <p>
              <EngineLabel
                details={`NeonGammon ${snapshot.package_version}`}
                tooltip={t('sageModelIdentity', {
                  fingerprint: snapshot.artifact_fingerprint,
                })}
                format={(version) =>
                  t(engine?.available ? 'engineVersion' : 'sageLastVersion', {
                    version,
                  })
                }
              />
            </p>
            {engine?.available === false && <p>{t('unavailable')}</p>}
            {engine?.available && engine.plies.length > 0 && (
              <p>
                {t('sageSupportedDepths', {
                  depths: formatDepths(engine.plies),
                })}
              </p>
            )}
            <p>
              {t('sageVerifiedDepths', {
                depths: formatDepths(
                  snapshot.one_ply_verified && snapshot.plies !== 1
                    ? [1, snapshot.plies]
                    : [snapshot.plies],
                ),
              })}
            </p>
          </>
        ) : (
          installation && <p>{t('sageUnverified')}</p>
        )}
      </div>
      <p className="settings-help" id={saveTargetId}>
        {t('verifySageTarget')}
      </p>
      {error !== undefined && (
        <div className="settings-error" role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={t(isConflict(error) ? 'loadLatest' : 'retry')}
            type="button"
            onClick={reload}
          />
        </div>
      )}
    </fieldset>
  );
}

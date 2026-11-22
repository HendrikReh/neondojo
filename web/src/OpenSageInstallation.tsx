import { useEffect, useId, useRef, useState } from 'react';
import {
  loadSageInstallation,
  saveSageInstallation,
  SettingsApiError,
  type SageInstallation,
} from './settings-api';
import { useI18n } from './shared/i18n/i18n';
import { useSettingsText } from './settings-messages';

const text = {
  de: [
    'Lokale NeonGammon-Installation',
    'Python-Pfad',
    'Prüfen und speichern',
    'Prüfung läuft…',
    'Geprüft: 2-ply. GNUbg verwaltet weiterhin Regeln und Spielstände.',
    'Installation nicht geprüft',
  ],
  en: [
    'Local NeonGammon installation',
    'Python path',
    'Verify and save',
    'Verifying…',
    'Verified: 2-ply. GNUbg continues to manage rules and saved games.',
    'Installation not verified',
  ],
  es: [
    'Instalación local de NeonGammon',
    'Ruta de Python',
    'Verificar y guardar',
    'Verificando…',
    'Verificado: 2-ply. GNUbg mantiene las reglas y las partidas guardadas.',
    'Instalación no verificada',
  ],
  fr: [
    'Installation locale NeonGammon',
    'Chemin Python',
    'Vérifier et enregistrer',
    'Vérification…',
    'Vérifié : 2-ply. GNUbg conserve les règles et les parties sauvegardées.',
    'Installation non vérifiée',
  ],
  it: [
    'Installazione locale NeonGammon',
    'Percorso Python',
    'Verifica e salva',
    'Verifica…',
    'Verificato: 2-ply. GNUbg gestisce ancora regole e partite salvate.',
    'Installazione non verificata',
  ],
  pl: [
    'Lokalna instalacja NeonGammon',
    'Ścieżka Pythona',
    'Sprawdź i zapisz',
    'Sprawdzanie…',
    'Zweryfikowano: 2-ply. GNUbg nadal zarządza regułami i zapisanymi grami.',
    'Instalacja niezweryfikowana',
  ],
};
export function useOpenSageInstallation(open: boolean, settingsReload: number) {
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
      }
    } catch (e) {
      if (!abort.signal.aborted) {
        setError(e);
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
    reload: () => setReload((value) => value + 1),
    discard: () => {
      setPath(installation?.python_path ?? '');
      setError(undefined);
    },
  };
}

export default function OpenSageInstallation({
  setup,
  onVerified,
}: {
  setup: ReturnType<typeof useOpenSageInstallation>;
  onVerified: () => void;
}) {
  const { locale, formatError } = useI18n();
  const t = text[locale];
  const settingsT = useSettingsText();
  const saveTargetId = useId();
  const { installation, path, setPath, busy, error, verify, reload } = setup;
  return (
    <fieldset className="settings-section settings-engine" disabled={busy}>
      <legend>{t[0]}</legend>
      <div className="settings-engine-path">
        <label className="settings-field">
          <span>{t[1]}</span>
          <input
            value={path}
            disabled={!installation}
            onChange={(e) => setPath(e.target.value)}
            spellCheck={false}
            autoComplete="off"
          />
        </label>
        <button
          type="button"
          aria-describedby={saveTargetId}
          disabled={!installation || !path.trim()}
          onClick={() => void verify().then(onVerified, () => {})}
        >
          {t[busy ? 3 : 2]}
        </button>
      </div>
      <p className="settings-help" id={saveTargetId}>
        {settingsT('verifySageTarget')}
      </p>
      <p className="settings-help" role="status">
        {t[installation?.snapshot && path === installation.python_path ? 4 : 5]}
      </p>
      {error !== undefined && (
        <>
          <p role="alert">{formatError(error)}</p>
          <button type="button" onClick={reload}>
            {settingsT(
              error instanceof SettingsApiError && error.status === 409
                ? 'loadLatest'
                : 'retry',
            )}
          </button>
        </>
      )}
    </fieldset>
  );
}

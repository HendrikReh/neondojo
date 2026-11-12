import { useEffect, useRef, useState } from 'react';
import {
  loadSageInstallation,
  saveSageInstallation,
  SettingsApiError,
  type SageInstallation,
} from './settings-api';
import { useI18n } from './i18n';
import { useSettingsText } from './settings-messages';

const text = {
  de: [
    'Lokale Open-Sage-Installation',
    'Python-Pfad',
    'Prüfen und speichern',
    'Prüfung läuft…',
    'Geprüft: 2-ply. GNUbg verwaltet weiterhin Regeln und Spielstände.',
    'Installation nicht geprüft',
  ],
  en: [
    'Local Open Sage installation',
    'Python path',
    'Verify and save',
    'Verifying…',
    'Verified: 2-ply. GNUbg continues to manage rules and saved games.',
    'Installation not verified',
  ],
  es: [
    'Instalación local de Open Sage',
    'Ruta de Python',
    'Verificar y guardar',
    'Verificando…',
    'Verificado: 2-ply. GNUbg mantiene las reglas y las partidas guardadas.',
    'Instalación no verificada',
  ],
  fr: [
    'Installation locale Open Sage',
    'Chemin Python',
    'Vérifier et enregistrer',
    'Vérification…',
    'Vérifié : 2-ply. GNUbg conserve les règles et les parties sauvegardées.',
    'Installation non vérifiée',
  ],
  it: [
    'Installazione locale Open Sage',
    'Percorso Python',
    'Verifica e salva',
    'Verifica…',
    'Verificato: 2-ply. GNUbg gestisce ancora regole e partite salvate.',
    'Installazione non verificata',
  ],
  pl: [
    'Lokalna instalacja Open Sage',
    'Ścieżka Pythona',
    'Sprawdź i zapisz',
    'Sprawdzanie…',
    'Zweryfikowano: 2-ply. GNUbg nadal zarządza regułami i zapisanymi grami.',
    'Instalacja niezweryfikowana',
  ],
};
export default function OpenSageInstallation({
  onVerified,
}: {
  onVerified: () => void;
}) {
  const { locale, formatError } = useI18n();
  const t = text[locale];
  const settingsT = useSettingsText();
  const [reload, setReload] = useState(0);
  const [installation, setInstallation] = useState<SageInstallation>();
  const [path, setPath] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const saveAbort = useRef<AbortController | null>(null);
  useEffect(() => {
    const abort = new AbortController();
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
    };
  }, [reload]);
  async function verify() {
    if (!installation || busy) return;
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
        onVerified();
      }
    } catch (e) {
      if (!abort.signal.aborted) setError(e);
    } finally {
      if (!abort.signal.aborted) setBusy(false);
    }
  }
  return (
    <fieldset className="settings-section" disabled={busy}>
      <legend>{t[0]}</legend>
      <label className="settings-field">
        <span>{t[1]}</span>
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          spellCheck={false}
        />
      </label>
      <button
        type="button"
        disabled={!installation || !path.trim()}
        onClick={() => void verify()}
      >
        {t[busy ? 3 : 2]}
      </button>
      <p className="settings-help" role="status">
        {t[installation?.snapshot && path === installation.python_path ? 4 : 5]}
      </p>
      {error !== undefined && (
        <>
          <p role="alert">{formatError(error)}</p>
          <button type="button" onClick={() => setReload((value) => value + 1)}>
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

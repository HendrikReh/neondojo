import { useCallback } from 'react';
import {
  ConflictError,
  HttpError,
  type ErrorIssue,
} from '../../shared/http/api';
import { formatLocalizedError, type Locale } from '../../shared/i18n/i18n-core';
import { useI18n } from '../../shared/i18n/i18n';
import { defineMessages, fromRows } from '../../shared/i18n/messages';
import { profileFieldKey, profileText } from './profile-messages';

/** Error codes of the configuration and profile endpoints and of this client. */
const errors = {
  duplicate_name: [
    'A profile with this name already exists.',
    'Ein Profil mit diesem Namen ist bereits vorhanden.',
    'Ya existe un perfil con este nombre.',
    'Un profil portant ce nom existe déjà.',
    'Profil o tej nazwie już istnieje.',
    'Esiste già un profilo con questo nome.',
  ],
  active_profile: [
    'Activate another profile before deleting this one.',
    'Vor dem Löschen ein anderes Profil aktivieren.',
    'Activa otro perfil antes de eliminar este.',
    'Activez un autre profil avant de supprimer celui-ci.',
    'Przed usunięciem tego profilu aktywuj inny.',
    'Attiva un altro profilo prima di eliminare questo.',
  ],
  profile_not_found: [
    'The profile no longer exists.',
    'Das Profil ist nicht mehr vorhanden.',
    'El perfil ya no existe.',
    'Le profil n’existe plus.',
    'Profil już nie istnieje.',
    'Il profilo non esiste più.',
  ],
  revision_conflict: [
    'The settings have changed in the meantime.',
    'Die Einstellungen wurden inzwischen geändert.',
    'Los ajustes han cambiado entretanto.',
    'Les réglages ont changé entre-temps.',
    'Ustawienia zmieniły się w międzyczasie.',
    'Le impostazioni sono cambiate nel frattempo.',
  ],
  unavailable_resources: [
    'Some of these settings are not available on this computer.',
    'Einige dieser Einstellungen sind auf diesem Computer nicht verfügbar.',
    'Algunos de estos ajustes no están disponibles en este ordenador.',
    'Certains de ces réglages ne sont pas disponibles sur cet ordinateur.',
    'Niektóre z tych ustawień są niedostępne na tym komputerze.',
    'Alcune di queste impostazioni non sono disponibili su questo computer.',
  ],
  invalid_configuration: [
    'The settings contain an invalid value.',
    'Die Einstellungen enthalten einen ungültigen Wert.',
    'Los ajustes contienen un valor no válido.',
    'Les réglages contiennent une valeur non valide.',
    'Ustawienia zawierają nieprawidłową wartość.',
    'Le impostazioni contengono un valore non valido.',
  ],
  internal_error: [
    'The backend could not process the settings.',
    'Das Backend konnte die Einstellungen nicht verarbeiten.',
    'El servidor no ha podido procesar los ajustes.',
    'Le serveur n’a pas pu traiter les réglages.',
    'Serwer nie mógł przetworzyć ustawień.',
    'Il server non è riuscito a elaborare le impostazioni.',
  ],
  uncertain_result: [
    'The connection was interrupted, and the settings have changed since then. Load the server settings.',
    'Die Verbindung wurde unterbrochen, und die Einstellungen haben sich seitdem geändert. Den Serverstand laden.',
    'Se interrumpió la conexión y los ajustes han cambiado desde entonces. Carga los ajustes del servidor.',
    'La connexion a été interrompue et les réglages ont changé depuis. Chargez les réglages du serveur.',
    'Połączenie zostało przerwane, a ustawienia od tego czasu się zmieniły. Wczytaj ustawienia serwera.',
    'La connessione è stata interrotta e le impostazioni sono cambiate da allora. Carica le impostazioni del server.',
  ],
  journal_conflict: [
    'Saved local changes conflict with newer settings.',
    'Gesicherte lokale Änderungen stehen im Konflikt mit neueren Einstellungen.',
    'Los cambios locales guardados entran en conflicto con ajustes más recientes.',
    'Les modifications locales enregistrées sont en conflit avec des réglages plus récents.',
    'Zapisane lokalne zmiany są w konflikcie z nowszymi ustawieniami.',
    'Le modifiche locali salvate sono in conflitto con impostazioni più recenti.',
  ],
  stale_profile: [
    'The active profile has changed. Review its current settings before resetting.',
    'Das aktive Profil wurde geändert. Vor dem Zurücksetzen die aktuellen Einstellungen prüfen.',
    'El perfil activo ha cambiado. Revisa sus ajustes actuales antes de restablecer.',
    'Le profil actif a changé. Vérifiez ses réglages actuels avant de réinitialiser.',
    'Aktywny profil się zmienił. Przed przywróceniem sprawdź jego bieżące ustawienia.',
    'Il profilo attivo è cambiato. Controlla le impostazioni attuali prima di ripristinare.',
  ],
  remote_conflict: [
    'The settings have changed in another window.',
    'Die Einstellungen wurden in einem anderen Fenster geändert.',
    'Los ajustes han cambiado en otra ventana.',
    'Les réglages ont changé dans une autre fenêtre.',
    'Ustawienia zmieniono w innym oknie.',
    'Le impostazioni sono cambiate in un’altra finestra.',
  ],
} as const;

const issues = {
  gnubg_unavailable: [
    'The configured GNUbg installation is not available.',
    'Die eingestellte GNUbg-Installation ist nicht verfügbar.',
    'La instalación configurada de GNUbg no está disponible.',
    'L’installation GNUbg configurée n’est pas disponible.',
    'Skonfigurowana instalacja GNUbg jest niedostępna.',
    'L’installazione GNUbg configurata non è disponibile.',
  ],
  bgsage_unavailable: [
    'The NeonGammon installation or the selected depth is unavailable or has changed.',
    'Die NeonGammon-Installation oder die gewählte Tiefe ist nicht verfügbar oder wurde geändert.',
    'La instalación de NeonGammon o la profundidad elegida no está disponible o ha cambiado.',
    'L’installation NeonGammon ou la profondeur choisie est indisponible ou a changé.',
    'Instalacja NeonGammon lub wybrana głębokość jest niedostępna lub się zmieniła.',
    'L’installazione NeonGammon o la profondità scelta non è disponibile o è cambiata.',
  ],
  engine_unavailable: [
    'The engine is not available.',
    'Die Engine ist nicht verfügbar.',
    'El motor no está disponible.',
    'Le moteur n’est pas disponible.',
    'Silnik jest niedostępny.',
    'Il motore non è disponibile.',
  ],
  book_unavailable: [
    'The selected opening book is not available.',
    'Das gewählte Eröffnungsbuch ist nicht verfügbar.',
    'El libro de aperturas elegido no está disponible.',
    'Le livre d’ouvertures choisi n’est pas disponible.',
    'Wybrana księga otwarć jest niedostępna.',
    'Il libro di aperture scelto non è disponibile.',
  ],
  legacy_reference_missing: [
    'The previous selection no longer exists; all entries are shown.',
    'Die frühere Auswahl ist nicht mehr vorhanden; es werden alle Einträge angezeigt.',
    'La selección anterior ya no existe; se muestran todas las entradas.',
    'La sélection précédente n’existe plus ; toutes les entrées sont affichées.',
    'Poprzedni wybór już nie istnieje; wyświetlane są wszystkie pozycje.',
    'La selezione precedente non esiste più; vengono mostrate tutte le voci.',
  ],
  invalid_value: [
    'Invalid value: {detail}',
    'Ungültiger Wert: {detail}',
    'Valor no válido: {detail}',
    'Valeur non valide : {detail}',
    'Nieprawidłowa wartość: {detail}',
    'Valore non valido: {detail}',
  ],
} as const;

const errorMessages = defineMessages('configuration-error', fromRows(errors));
const issueMessages = defineMessages('configuration-issue', fromRows(issues));

type ErrorCode = keyof typeof errors;
type IssueCode = keyof typeof issues;

export const hasNameIssue = (error: unknown) =>
  error instanceof HttpError &&
  (error.code === 'duplicate_name' ||
    error.issues.some((issue) => issue.path === 'name'));

/** The message of an error in the interface language, without its issues. */
export function configurationErrorText(locale: Locale, error: unknown) {
  if (error instanceof HttpError || error instanceof ConflictError) {
    if (error.code !== 'duplicate_name' && hasNameIssue(error))
      return profileText(locale, 'invalidName');
    if (Object.hasOwn(errors, error.code))
      return errorMessages.text(locale, error.code as ErrorCode);
  }
  return formatLocalizedError(locale, error);
}

/** What is wrong with a configuration value, in the interface language. */
export function configurationIssueText(locale: Locale, issue: ErrorIssue) {
  const code =
    issue.code === 'engine_unavailable' && issue.engine
      ? `${issue.engine}_unavailable`
      : issue.code;
  return Object.hasOwn(issues, code)
    ? issueMessages.text(locale, code as IssueCode, { detail: issue.message })
    : issue.message;
}

/** The section and name of a configuration value, for example "Play · Engine". */
export function configurationPathText(locale: Locale, path: string) {
  const parts = path.split('.');
  return [...new Set([parts[0], parts.at(-1)!])]
    .map((part) => {
      const key = profileFieldKey(part);
      return key ? profileText(locale, key) : part;
    })
    .join(' · ');
}

export function useConfigurationErrors() {
  const { locale } = useI18n();
  return {
    message: useCallback(
      (error: unknown) => configurationErrorText(locale, error),
      [locale],
    ),
    /** A configuration value and what is wrong with it. */
    issue: useCallback(
      (issue: ErrorIssue) =>
        `${configurationPathText(locale, issue.path)}: ${configurationIssueText(locale, issue)}`,
      [locale],
    ),
    issues: (error: unknown): readonly ErrorIssue[] =>
      // The name dialog and message already say what is wrong with a name.
      error instanceof HttpError && !hasNameIssue(error) ? error.issues : [],
  };
}

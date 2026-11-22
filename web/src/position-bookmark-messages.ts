import { useI18n, type Locale } from './shared/i18n/i18n';

const entries = {
  save: [
    'Save position',
    'Position merken',
    'Guardar posición',
    'Enregistrer la position',
    'Zapisz pozycję',
    'Salva posizione',
  ],
  remove: [
    'Remove from collection',
    'Aus Sammlung entfernen',
    'Quitar de la colección',
    'Retirer de la collection',
    'Usuń z kolekcji',
    'Rimuovi dalla raccolta',
  ],
  loading: [
    'Checking collection…',
    'Sammlung wird geprüft…',
    'Comprobando colección…',
    'Vérification de la collection…',
    'Sprawdzanie kolekcji…',
    'Verifica della raccolta…',
  ],
  saving: [
    'Saving…',
    'Wird gespeichert…',
    'Guardando…',
    'Enregistrement…',
    'Zapisywanie…',
    'Salvataggio…',
  ],
  removing: [
    'Removing…',
    'Wird entfernt…',
    'Quitando…',
    'Retrait…',
    'Usuwanie…',
    'Rimozione…',
  ],
  loadError: [
    'Could not load collection status.',
    'Sammlungsstatus konnte nicht geladen werden.',
    'No se pudo cargar el estado de la colección.',
    'Impossible de charger le statut de la collection.',
    'Nie udało się wczytać stanu kolekcji.',
    'Impossibile caricare lo stato della raccolta.',
  ],
  saveError: [
    'Could not save position.',
    'Position konnte nicht gespeichert werden.',
    'No se pudo guardar la posición.',
    'Impossible d’enregistrer la position.',
    'Nie udało się zapisać pozycji.',
    'Impossibile salvare la posizione.',
  ],
  removeError: [
    'Could not remove position.',
    'Position konnte nicht entfernt werden.',
    'No se pudo quitar la posición.',
    'Impossible de retirer la position.',
    'Nie udało się usunąć pozycji.',
    'Impossibile rimuovere la posizione.',
  ],
  retry: [
    'Retry',
    'Erneut versuchen',
    'Reintentar',
    'Réessayer',
    'Spróbuj ponownie',
    'Riprova',
  ],
} satisfies Record<string, [string, string, string, string, string, string]>;
type Key = keyof typeof entries;
const locales: Locale[] = ['en', 'de', 'es', 'fr', 'pl', 'it'];
export const positionBookmarkMessages = Object.fromEntries(
  locales.map((locale, i) => [
    locale,
    Object.fromEntries(
      Object.entries(entries).map(([key, values]) => [key, values[i]]),
    ),
  ]),
) as Record<Locale, Record<Key, string>>;

export function usePositionBookmarkText() {
  const { locale } = useI18n();
  return (key: Key) => positionBookmarkMessages[locale][key];
}

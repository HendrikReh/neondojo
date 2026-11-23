import { defineMessages } from '../../../shared/i18n/messages';

const en = {
  rename: 'Rename deck',
  delete: 'Delete deck',
  save: 'Save name',
  cancel: 'Cancel',
  name: 'Deck name',
  details: '{name} · {count, plural, one {# card} other {# cards}}',
  keep: 'Saved positions, learning cards and review history remain. Other decks keep their cards. Adding the same decision to a deck again restores its learning progress.',
  confirm:
    'Delete “{name}” with {count, plural, one {# card} other {# cards}} from my decks.',
  changed:
    'This deck changed or was removed elsewhere. Refresh its details before trying again. Your name draft is kept.',
  refresh: 'Refresh deck details',
  missing:
    'This deck no longer exists. Close this dialog and choose another deck.',
  working: 'Saving…',
  refreshed: 'Latest deck details loaded. Review them before confirming.',
};
type Key = keyof typeof en;
type Catalog = Record<Key, string>;
const de: Catalog = {
  rename: 'Deck umbenennen',
  delete: 'Deck löschen',
  save: 'Namen speichern',
  cancel: 'Abbrechen',
  name: 'Deckname',
  details: '{name} · {count, plural, one {# Karte} other {# Karten}}',
  keep: 'Gespeicherte Positionen, Lernkarten und Wiederholungsverlauf bleiben erhalten. Andere Decks behalten ihre Karten. Wenn du dieselbe Entscheidung erneut einem Deck hinzufügst, wird ihr Lernfortschritt wiederhergestellt.',
  confirm:
    '„{name}“ mit {count, plural, one {# Karte} other {# Karten}} aus meinen Decks löschen.',
  changed:
    'Dieses Deck wurde anderswo geändert oder entfernt. Aktualisiere seine Angaben vor einem erneuten Versuch. Dein Namensentwurf bleibt erhalten.',
  refresh: 'Deckangaben aktualisieren',
  missing:
    'Dieses Deck existiert nicht mehr. Schließe diesen Dialog und wähle ein anderes Deck.',
  working: 'Wird gespeichert …',
  refreshed: 'Aktuelle Deckangaben geladen. Prüfe sie vor der Bestätigung.',
};
const es: Catalog = {
  rename: 'Renombrar mazo',
  delete: 'Eliminar mazo',
  save: 'Guardar nombre',
  cancel: 'Cancelar',
  name: 'Nombre del mazo',
  details: '{name} · {count, plural, one {# tarjeta} other {# tarjetas}}',
  keep: 'Las posiciones guardadas, las tarjetas y el historial de repasos se conservan. Los demás mazos conservan sus tarjetas. Al añadir de nuevo la misma decisión a un mazo se recupera su progreso.',
  confirm:
    'Eliminar «{name}» con {count, plural, one {# tarjeta} other {# tarjetas}} de mis mazos.',
  changed:
    'Este mazo se ha modificado o eliminado en otro lugar. Actualiza sus datos antes de volver a intentarlo. Se conserva el borrador del nombre.',
  refresh: 'Actualizar datos del mazo',
  missing: 'Este mazo ya no existe. Cierra este diálogo y elige otro mazo.',
  working: 'Guardando…',
  refreshed: 'Datos actualizados. Revísalos antes de confirmar.',
};
const fr: Catalog = {
  rename: 'Renommer le paquet',
  delete: 'Supprimer le paquet',
  save: 'Enregistrer le nom',
  cancel: 'Annuler',
  name: 'Nom du paquet',
  details: '{name} · {count, plural, one {# carte} other {# cartes}}',
  keep: 'Les positions enregistrées, les cartes et l’historique des révisions sont conservés. Les autres paquets conservent leurs cartes. Ajouter à nouveau la même décision à un paquet restaure sa progression.',
  confirm:
    'Supprimer « {name} » avec {count, plural, one {# carte} other {# cartes}} de mes paquets.',
  changed:
    'Ce paquet a été modifié ou supprimé ailleurs. Actualisez ses données avant de réessayer. Votre nom proposé est conservé.',
  refresh: 'Actualiser le paquet',
  missing:
    'Ce paquet n’existe plus. Fermez cette fenêtre et choisissez un autre paquet.',
  working: 'Enregistrement…',
  refreshed: 'Données actualisées. Vérifiez-les avant de confirmer.',
};
const it: Catalog = {
  rename: 'Rinomina mazzo',
  delete: 'Elimina mazzo',
  save: 'Salva nome',
  cancel: 'Annulla',
  name: 'Nome del mazzo',
  details: '{name} · {count, plural, one {# scheda} other {# schede}}',
  keep: 'Le posizioni salvate, le schede e la cronologia dei ripassi restano disponibili. Gli altri mazzi conservano le loro schede. Aggiungere di nuovo la stessa decisione a un mazzo ripristina i suoi progressi.',
  confirm:
    'Elimina «{name}» con {count, plural, one {# scheda} other {# schede}} dai miei mazzi.',
  changed:
    'Questo mazzo è stato modificato o eliminato altrove. Aggiorna i dati prima di riprovare. La bozza del nome viene conservata.',
  refresh: 'Aggiorna dati del mazzo',
  missing:
    'Questo mazzo non esiste più. Chiudi questa finestra e scegli un altro mazzo.',
  working: 'Salvataggio…',
  refreshed: 'Dati aggiornati. Controllali prima di confermare.',
};
const pl: Catalog = {
  rename: 'Zmień nazwę talii',
  delete: 'Usuń talię',
  save: 'Zapisz nazwę',
  cancel: 'Anuluj',
  name: 'Nazwa talii',
  details:
    '{name} · {count, plural, one {# fiszka} few {# fiszki} many {# fiszek}}',
  keep: 'Zapisane pozycje, fiszki i historia powtórek pozostają. Pozostałe talie zachowują swoje fiszki. Ponowne dodanie tej samej decyzji do talii przywraca jej postępy w nauce.',
  confirm:
    'Usuń „{name}” z {count, plural, one {# fiszką} few {# fiszkami} many {# fiszkami}} z moich talii.',
  changed:
    'Ta talia została zmieniona lub usunięta w innym miejscu. Odśwież jej dane przed kolejną próbą. Szkic nazwy zostaje zachowany.',
  refresh: 'Odśwież dane talii',
  missing: 'Ta talia już nie istnieje. Zamknij to okno i wybierz inną talię.',
  working: 'Zapisywanie…',
  refreshed: 'Wczytano aktualne dane. Sprawdź je przed potwierdzeniem.',
};
const messages = defineMessages('deck-management', { en, de, es, fr, pl, it });
export const deckManagementMessages = messages.catalogs;
export const useDeckManagementText = messages.useText;

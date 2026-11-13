import { useI18n } from './i18n';
import { interpolate, type Locale, type TranslationParams } from './i18n-core';

const rows = {
  ankiImport: [
    'Import BG Tutor Anki deck',
    'BG-Tutor-Anki-Deck importieren',
    'Importar mazo BG Tutor de Anki',
    'Importer un paquet Anki BG Tutor',
    'Importuj talię Anki BG Tutor',
    'Importa mazzo Anki BG Tutor',
  ],
  ankiImported: [
    '{added} new · {existing} already in the collection',
    '{added} neu · {existing} bereits in der Sammlung',
    '{added} nuevas · {existing} ya en la colección',
    '{added} nouvelles · {existing} déjà dans la collection',
    '{added} nowych · {existing} już w kolekcji',
    '{added} nuove · {existing} già nella raccolta',
  ],
  externalAnkiImport: [
    'Import Anki package',
    'Anki-Paket importieren',
    'Importar paquete de Anki',
    'Importer un paquet Anki',
    'Importuj pakiet Anki',
    'Importa pacchetto Anki',
  ],
  externalAnkiImported: [
    'Cards imported {cards} · Decks {decks}',
    'Importierte Karten {cards} · Decks {decks}',
    'Tarjetas importadas {cards} · Mazos {decks}',
    'Cartes importées {cards} · Paquets {decks}',
    'Zaimportowane fiszki {cards} · Talie {decks}',
    'Schede importate {cards} · Mazzi {decks}',
  ],
  ankiImporting: [
    'Importing Anki package…',
    'Anki-Paket wird importiert…',
    'Importando paquete de Anki…',
    'Importation du paquet Anki…',
    'Importowanie pakietu Anki…',
    'Importazione del pacchetto Anki…',
  ],
} as const;

const languages: readonly Locale[] = ['en', 'de', 'es', 'fr', 'pl', 'it'];
export function useAnkiImportText() {
  const { locale } = useI18n();
  return (key: keyof typeof rows, params: TranslationParams = {}) =>
    interpolate(rows[key][languages.indexOf(locale)] ?? rows[key][0], params);
}

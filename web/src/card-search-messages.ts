import { useI18n } from './i18n';
import { interpolate, type Locale, type TranslationParams } from './i18n-core';
const rows = {
  resizePreview: [
    'Resize card preview',
    'Breite der Kartenvorschau ändern',
    'Cambiar el ancho de la vista previa',
    'Redimensionner l’aperçu de la carte',
    'Zmień szerokość podglądu karty',
    'Ridimensiona l’anteprima della scheda',
  ],
  resizePreviewHelp: [
    'Drag or use Left/Right arrows. Double-click or Enter resets the width.',
    'Ziehen oder Pfeiltasten links/rechts nutzen. Doppelklick oder Enter setzt die Breite zurück.',
    'Arrastra o usa las flechas izquierda/derecha. Doble clic o Intro restablece el ancho.',
    'Glissez ou utilisez les flèches gauche/droite. Double-clic ou Entrée rétablit la largeur.',
    'Przeciągnij lub użyj strzałek lewo/prawo. Dwuklik lub Enter przywraca szerokość.',
    'Trascina o usa le frecce sinistra/destra. Doppio clic o Invio ripristina la larghezza.',
  ],
  withoutDeck: [
    'Without a deck',
    'Ohne Deck',
    'Sin mazo',
    'Sans paquet',
    'Bez talii',
    'Senza mazzo',
  ],
  allCards: [
    'All flashcards',
    'Alle Lernkarten',
    'Todas las tarjetas',
    'Toutes les cartes',
    'Wszystkie fiszki',
    'Tutte le schede',
  ],
  search: [
    'Search cards',
    'Lernkarten suchen',
    'Buscar tarjetas',
    'Rechercher des cartes',
    'Szukaj fiszek',
    'Cerca schede',
  ],
  found: [
    '{count} cards found',
    '{count} Lernkarten gefunden',
    '{count} tarjetas encontradas',
    '{count} cartes trouvées',
    'Znaleziono {count} fiszek',
    '{count} schede trovate',
  ],
  noResults: [
    'No flashcards match these filters.',
    'Keine Lernkarten entsprechen diesen Filtern.',
    'Ninguna tarjeta coincide con estos filtros.',
    'Aucune carte ne correspond à ces filtres.',
    'Żadne fiszki nie pasują do tych filtrów.',
    'Nessuna scheda corrisponde a questi filtri.',
  ],
  kind: [
    'Card type',
    'Kartentyp',
    'Tipo de tarjeta',
    'Type de carte',
    'Rodzaj fiszki',
    'Tipo di scheda',
  ],
  allKinds: [
    'All card types',
    'Alle Kartentypen',
    'Todos los tipos',
    'Tous les types',
    'Wszystkie rodzaje',
    'Tutti i tipi',
  ],
  anki: [
    'Anki card',
    'Anki-Karte',
    'Tarjeta Anki',
    'Carte Anki',
    'Fiszka Anki',
    'Scheda Anki',
  ],
  checker: [
    'Checker play',
    'Steinzug',
    'Jugada de fichas',
    'Déplacement des pions',
    'Ruch pionami',
    'Mossa delle pedine',
  ],
  cube_offer: [
    'Double / no double',
    'Doppeln / nicht doppeln',
    'Doblar / no doblar',
    'Doubler / ne pas doubler',
    'Podwoić / nie podwajać',
    'Raddoppiare / non raddoppiare',
  ],
  cube_response: [
    'Take / pass',
    'Annehmen / passen',
    'Aceptar / pasar',
    'Prendre / passer',
    'Przyjąć / spasować',
    'Accettare / passare',
  ],
  status: [
    'Review status',
    'Lernstatus',
    'Estado de repaso',
    'État des révisions',
    'Stan powtórek',
    'Stato dei ripassi',
  ],
  allStatuses: [
    'Any review status',
    'Alle Lernstände',
    'Cualquier estado',
    'Tous les états',
    'Wszystkie stany',
    'Qualsiasi stato',
  ],
  new: ['New', 'Neu', 'Nueva', 'Nouvelle', 'Nowa', 'Nuova'],
  due: [
    'Due',
    'Fällig',
    'Pendiente',
    'À réviser',
    'Do powtórki',
    'Da ripassare',
  ],
  reviewed: [
    'Reviewed',
    'Bereits wiederholt',
    'Repasada',
    'Déjà révisée',
    'Powtórzona',
    'Già ripassata',
  ],
  scopeHint: [
    'Filters affect this list. Study and export use the selected deck.',
    'Filter gelten für diese Liste. Lernen und Export verwenden das ausgewählte Deck.',
    'Los filtros afectan a esta lista. El estudio y la exportación usan el mazo seleccionado.',
    'Les filtres concernent cette liste. L’étude et l’export utilisent le paquet sélectionné.',
    'Filtry dotyczą tej listy. Nauka i eksport używają wybranej talii.',
    'I filtri riguardano questo elenco. Lo studio e l’esportazione usano il mazzo selezionato.',
  ],
} as const;
export type CardSearchMessage = keyof typeof rows;
const locales: Locale[] = ['en', 'de', 'es', 'fr', 'pl', 'it'];
export function cardSearchText(
  locale: Locale,
  key: CardSearchMessage,
  params: TranslationParams = {},
) {
  return interpolate(
    rows[key][locales.indexOf(locale)],
    Object.fromEntries(
      Object.entries(params).map(([k, v]) => [
        k,
        typeof v === 'number' ? new Intl.NumberFormat(locale).format(v) : v,
      ]),
    ),
  );
}
export function useCardSearchText() {
  const { locale } = useI18n();
  return (key: CardSearchMessage, params: TranslationParams = {}) =>
    cardSearchText(locale, key, params);
}

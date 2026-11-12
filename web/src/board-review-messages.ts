import { useI18n, type Locale } from './i18n';
import { interpolate, type TranslationParams } from './i18n-core';
const entries = {
  resizeBoard: [
    'Resize analysis board',
    'Brettgröße ändern',
    'Cambiar tamaño del tablero',
    'Redimensionner le plateau',
    'Zmień rozmiar planszy',
    'Ridimensiona tavola di analisi',
  ],
  resizeBoardHelp: [
    'Drag or use Left/Right arrows. Shift changes faster. Double-click or Enter resets the width.',
    'Ziehen oder Pfeiltasten links/rechts nutzen. Umschalt ändert schneller. Doppelklick oder Enter setzt die Breite zurück.',
    'Arrastra o usa las flechas izquierda/derecha. Mayús cambia más rápido. Doble clic o Intro restablece el ancho.',
    'Glissez ou utilisez les flèches gauche/droite. Maj accélère. Double-clic ou Entrée rétablit la largeur.',
    'Przeciągnij lub użyj strzałek lewo/prawo. Shift przyspiesza. Dwuklik lub Enter przywraca szerokość.',
    'Trascina o usa le frecce sinistra/destra. Maiusc accelera. Doppio clic o Invio ripristina la larghezza.',
  ],
  pipsLabel: [
    '{white} {whitePips} pips, {black} {blackPips} pips',
    '{white} {whitePips} Pips, {black} {blackPips} Pips',
    '{white} {whitePips} pips, {black} {blackPips} pips',
    '{white} {whitePips} pips, {black} {blackPips} pips',
    '{white} {whitePips} pipsów, {black} {blackPips} pipsów',
    '{white} {whitePips} pip, {black} {blackPips} pip',
  ],
  views: [
    'Board view',
    'Brettansicht',
    'Vista del tablero',
    'Vue du plateau',
    'Widok planszy',
    'Vista della tavola',
  ],
  before: ['Initial', 'Initial', 'Initial', 'Initial', 'Initial', 'Initial'],
  arrows: ['Arrows', 'Pfeile', 'Flechas', 'Flèches', 'Strzałki', 'Frecce'],
  after: ['Final', 'Final', 'Final', 'Final', 'Final', 'Final'],
  reset: [
    'Reset preview',
    'Zum gespielten Zug',
    'Restablecer vista previa',
    'Réinitialiser l’aperçu',
    'Resetuj podgląd',
    'Reimposta anteprima',
  ],
  played: [
    'Played move',
    'Gespielter Zug',
    'Jugada realizada',
    'Coup joué',
    'Zagrany ruch',
    'Mossa giocata',
  ],
  candidate: [
    'Candidate {rank} · {move}',
    'Kandidat {rank} · {move}',
    'Candidata {rank} · {move}',
    'Candidat {rank} · {move}',
    'Kandydat {rank} · {move}',
    'Candidata {rank} · {move}',
  ],
  previewMove: [
    'Preview move {rank}: {move}',
    'Zug {rank} ansehen: {move}',
    'Ver jugada {rank}: {move}',
    'Voir le coup {rank} : {move}',
    'Podgląd ruchu {rank}: {move}',
    'Anteprima mossa {rank}: {move}',
  ],
  unavailable: [
    'This move cannot be previewed safely. The stored position remains available.',
    'Dieser Zug kann nicht sicher dargestellt werden. Die gespeicherte Stellung bleibt verfügbar.',
    'No se puede mostrar esta jugada de forma segura. La posición guardada sigue disponible.',
    'Ce coup ne peut pas être prévisualisé de façon fiable. La position enregistrée reste disponible.',
    'Nie można bezpiecznie wyświetlić tego ruchu. Zapisana pozycja jest nadal dostępna.',
    'Impossibile mostrare questa mossa in modo affidabile. La posizione salvata resta disponibile.',
  ],
  analysisTarget: [
    'Analyses and notes refer to the position before this move.',
    'Analysen und Notizen beziehen sich auf die Stellung vor diesem Zug.',
    'Los análisis y las notas se refieren a la posición anterior a esta jugada.',
    'Les analyses et notes concernent la position avant ce coup.',
    'Analizy i notatki dotyczą pozycji przed tym ruchem.',
    'Analisi e note si riferiscono alla posizione prima di questa mossa.',
  ],
  arrowsHelp: [
    'Arrows show the selected move, including intermediate points.',
    'Die Pfeile zeigen den gewählten Zug mit Zwischenpunkten.',
    'Las flechas muestran la jugada seleccionada y sus puntos intermedios.',
    'Les flèches montrent le coup sélectionné et ses points intermédiaires.',
    'Strzałki pokazują wybrany ruch i punkty pośrednie.',
    'Le frecce mostrano la mossa scelta e i punti intermedi.',
  ],
  pass: [
    'No checker can move.',
    'Kein Stein kann ziehen.',
    'No se puede mover ninguna ficha.',
    'Aucun pion ne peut bouger.',
    'Żaden pion nie może się ruszyć.',
    'Nessuna pedina può muovere.',
  ],
  showing: [
    '{view} · {player} · {move}',
    '{view} · {player} · {move}',
    '{view} · {player} · {move}',
    '{view} · {player} · {move}',
    '{view} · {player} · {move}',
    '{view} · {player} · {move}',
  ],
} satisfies Record<string, [string, string, string, string, string, string]>;
type Key = keyof typeof entries;
const locales: Locale[] = ['en', 'de', 'es', 'fr', 'pl', 'it'];
export const boardReviewMessages = Object.fromEntries(
  locales.map((locale, i) => [
    locale,
    Object.fromEntries(
      Object.entries(entries).map(([key, values]) => [key, values[i]]),
    ),
  ]),
) as Record<Locale, Record<Key, string>>;
export const boardReviewText = (
  locale: Locale,
  key: Key,
  params: TranslationParams = {},
) => interpolate(boardReviewMessages[locale][key], params);
export function useBoardReviewText() {
  const { locale } = useI18n();
  return (key: Key, params?: TranslationParams) =>
    boardReviewText(locale, key, params);
}

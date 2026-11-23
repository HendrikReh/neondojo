import { useI18n } from '../../../shared/i18n/i18n';
import {
  defineMessages,
  fromRows,
  type MessageRow,
} from '../../../shared/i18n/messages';

const rows = {
  compareMoves: [
    'Compare moves',
    'Zugvergleich',
    'Comparar jugadas',
    'Comparer les coups',
    'Porównaj ruchy',
    'Confronta mosse',
  ],
  comparisonSource: [
    '{engine} analysis',
    '{engine} – Analyse',
    'Análisis de {engine}',
    'Analyse {engine}',
    'Analiza {engine}',
    'Analisi {engine}',
  ],
  xgComparison: [
    'Δ shows engine equity minus XG. Arrows mark a higher or lower rank than XG.',
    'Δ zeigt Engine-Equity minus XG. Pfeile markieren einen besseren oder schlechteren Rang als bei XG.',
    'Δ muestra la equidad del motor menos la de XG. Las flechas indican un rango mejor o peor que el de XG.',
    'Δ indique l’équité du moteur moins celle de XG. Les flèches signalent un rang meilleur ou moins bon que celui de XG.',
    'Δ to equity silnika minus equity XG. Strzałki oznaczają lepszą lub gorszą pozycję względem XG.',
    'Δ indica l’equity del motore meno quella di XG. Le frecce indicano un rango migliore o peggiore di quello di XG.',
  ],
  sortByXgDifference: [
    'Sort {engine} by absolute equity difference to XG',
    'Nach Betrag der Equity-Differenz zu XG sortieren ({engine})',
    'Ordenar {engine} por diferencia absoluta de equidad respecto a XG',
    'Trier {engine} par écart absolu d’équité avec XG',
    'Sortuj {engine} według wartości bezwzględnej różnicy equity względem XG',
    'Ordina {engine} per differenza assoluta di equity rispetto a XG',
  ],
  xgRank: [
    'XG equity rank {rank}',
    'XG-Equity-Rang {rank}',
    'Rango por equidad de XG {rank}',
    'Rang selon l’équité de XG {rank}',
    'Miejsce XG według equity {rank}',
    'Rango XG per equity {rank}',
  ],
  noCandidates: [
    'No candidate values recorded.',
    'Keine Kandidatenwerte aufgezeichnet.',
    'No hay valores de candidatas registrados.',
    'Aucune valeur de candidat enregistrée.',
    'Brak zapisanych ocen kandydatów.',
    'Nessun valore dei candidati registrato.',
  ],
  normalizedCubeful: [
    'Normalized cubeful',
    'Normierte Cubeful-Equity',
    'Cubeful normalizada',
    'Cubeful normalisée',
    'Znormalizowane cubeful',
    'Cubeful normalizzata',
  ],
  loss: ['Loss', 'Verlust', 'Pérdida', 'Perte', 'Strata', 'Perdita'],
  outcomeWins: [
    'Wins',
    'Gewinne',
    'Victorias',
    'Victoires',
    'Wygrane',
    'Vittorie',
  ],
  outcomeLosses: [
    'Losses',
    'Niederlagen',
    'Derrotas',
    'Défaites',
    'Przegrane',
    'Sconfitte',
  ],
  gameWin: ['Win', 'Gewinn', 'Victoria', 'Victoire', 'Wygrana', 'Vittoria'],
  gameLoss: [
    'Loss',
    'Niederlage',
    'Derrota',
    'Défaite',
    'Przegrana',
    'Sconfitta',
  ],
  outcome_total: ['Total', 'Gesamt', 'Total', 'Total', 'Razem', 'Totale'],
  outcome_gammon: ['Gammon', 'Gammon', 'Gammon', 'Gammon', 'Gammon', 'Gammon'],
  outcome_backgammon: [
    'Backgammon',
    'Backgammon',
    'Backgammon',
    'Backgammon',
    'Backgammon',
    'Backgammon',
  ],
  outcomeHelp: [
    'Total includes gammons and backgammons. Gammon excludes backgammons. — means unavailable.',
    'Gesamt enthält Gammons und Backgammons. Gammon zählt ohne Backgammons. — bedeutet nicht verfügbar.',
    'Total incluye gammons y backgammons. Gammon no incluye backgammons. — indica que no hay datos.',
    'Total inclut gammons et backgammons. Gammon exclut les backgammons. — indique une valeur indisponible.',
    'Razem obejmuje gammony i backgammony. Gammon nie obejmuje backgammonów. — oznacza brak danych.',
    'Totale include gammon e backgammon. Gammon esclude i backgammon. — indica un dato non disponibile.',
  ],
  lossHelp: [
    'Difference from rank 1 in the same unit; — means unavailable or not comparable.',
    'Abstand zu Rang 1 in derselben Einheit; — bedeutet nicht verfügbar oder nicht vergleichbar.',
    'Diferencia respecto al rango 1 en la misma unidad; — indica no disponible o no comparable.',
    'Écart au rang 1 dans la même unité ; — indique une valeur indisponible ou non comparable.',
    'Różnica względem miejsca 1 w tej samej jednostce; — oznacza brak lub nieporównywalne wartości.',
    'Differenza dal rango 1 nella stessa unità; — indica valori non disponibili o non confrontabili.',
  ],
  played: ['Played', 'Gespielt', 'Jugada', 'Joué', 'Zagrany', 'Giocata'],
  best: ['Best', 'Bester', 'Mejor', 'Meilleur', 'Najlepszy', 'Migliore'],
  preview: [
    'Preview',
    'Vorschau',
    'Vista previa',
    'Aperçu',
    'Podgląd',
    'Anteprima',
  ],
  perspective: [
    'Values for {player}',
    'Werte für {player}',
    'Valores para {player}',
    'Valeurs pour {player}',
    'Wartości dla {player}',
    'Valori per {player}',
  ],
  onRoll: [
    'player on roll',
    'Spieler am Zug',
    'jugador al turno',
    'joueur au trait',
    'gracza na ruchu',
    'giocatore di turno',
  ],
  unknownUnit: [
    'Unit unknown',
    'Einheit unbekannt',
    'Unidad desconocida',
    'Unité inconnue',
    'Nieznana jednostka',
    'Unità sconosciuta',
  ],
  mixedUnits: [
    'Units per row',
    'Einheiten je Zeile',
    'Unidades por fila',
    'Unités par ligne',
    'Jednostki w wierszach',
    'Unità per riga',
  ],
  percentagePoints: [
    '{value} pp',
    '{value} Pp.',
    '{value} pp',
    '{value} pp',
    '{value} pp',
    '{value} pp',
  ],
} satisfies Record<string, MessageRow>;

const messages = defineMessages('candidate', fromRows(rows));

export const candidateMessages = messages.catalogs;
export const useCandidateText = messages.useText;

export function useCandidateLossText() {
  const { formatNumber } = useI18n();
  const text = useCandidateText();
  const digits = { minimumFractionDigits: 3, maximumFractionDigits: 3 };
  return (loss: number | null, unit?: string) =>
    loss === null
      ? '—'
      : unit === 'mwc'
        ? text('percentagePoints', { value: formatNumber(loss * 100, digits) })
        : formatNumber(loss, digits);
}

export function useCandidateEquityText(digits = 3) {
  const { formatEquity, formatPercent } = useI18n();
  const text = useCandidateText();
  const unit = (kind: string) =>
    ({
      cubeful: 'Cubeful',
      cubeless: 'Cubeless',
      normalized_cubeful: text('normalizedCubeful'),
      mwc: 'MWC',
    })[kind] ?? text('unknownUnit');
  const equity = (value: number | null | undefined, kind: string) =>
    typeof value === 'number' && Number.isFinite(value)
      ? kind === 'mwc'
        ? formatPercent(value, digits)
        : formatEquity(value, digits)
      : '—';
  return { unit, equity };
}

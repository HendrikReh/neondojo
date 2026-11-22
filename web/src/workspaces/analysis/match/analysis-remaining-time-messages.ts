import type { Locale } from '../../../shared/i18n/i18n';
import {
  formatMessage,
  type TranslationParams,
} from '../../../shared/i18n/i18n-core';
const en = {
  estimated: 'Estimated time remaining: approx. {duration}',
  loading: 'Calculating time remaining …',
  queued: 'Waiting for analysis to start',
  waiting: 'Waiting for engine availability',
  reestimating: 'Recalculating time remaining …',
  unavailable: 'Remaining time is not yet available',
  finishing: 'Saving analysis results …',
  benchmark: 'Based on the saved benchmark',
  observed: 'Based on the measured analysis speed',
};
type Key = keyof typeof en;
export const remainingTimeMessages = {
  en,
  de: {
    estimated: 'Geschätzte Restzeit: ca. {duration}',
    loading: 'Restzeit wird ermittelt …',
    queued: 'Wartet auf Analysebeginn',
    waiting: 'Wartet auf Engineverfügbarkeit',
    reestimating: 'Restzeit wird neu geschätzt …',
    unavailable: 'Restzeit ist noch nicht verfügbar',
    finishing: 'Analyseergebnisse werden gespeichert …',
    benchmark: 'Auf Basis des gespeicherten Benchmarks',
    observed: 'Auf Basis des gemessenen Analysetempos',
  },
  es: {
    estimated: 'Tiempo restante estimado: aprox. {duration}',
    loading: 'Calculando el tiempo restante …',
    queued: 'Esperando a que empiece el análisis',
    waiting: 'Esperando a que el motor esté disponible',
    reestimating: 'Recalculando el tiempo restante …',
    unavailable: 'El tiempo restante aún no está disponible',
    finishing: 'Guardando los resultados del análisis …',
    benchmark: 'Según el benchmark guardado',
    observed: 'Según la velocidad de análisis medida',
  },
  fr: {
    estimated: 'Temps restant estimé : env. {duration}',
    loading: 'Calcul du temps restant …',
    queued: 'En attente du début de l’analyse',
    waiting: 'En attente de la disponibilité du moteur',
    reestimating: 'Nouveau calcul du temps restant …',
    unavailable: 'Le temps restant n’est pas encore disponible',
    finishing: 'Enregistrement des résultats de l’analyse …',
    benchmark: 'D’après le benchmark enregistré',
    observed: 'D’après la vitesse d’analyse mesurée',
  },
  it: {
    estimated: 'Tempo rimanente stimato: circa {duration}',
    loading: 'Calcolo del tempo rimanente …',
    queued: 'In attesa dell’avvio dell’analisi',
    waiting: 'In attesa che il motore sia disponibile',
    reestimating: 'Ricalcolo del tempo rimanente …',
    unavailable: 'Il tempo rimanente non è ancora disponibile',
    finishing: 'Salvataggio dei risultati dell’analisi …',
    benchmark: 'In base al benchmark salvato',
    observed: 'In base alla velocità di analisi misurata',
  },
  pl: {
    estimated: 'Szacowany pozostały czas: ok. {duration}',
    loading: 'Obliczanie pozostałego czasu …',
    queued: 'Oczekiwanie na rozpoczęcie analizy',
    waiting: 'Oczekiwanie na dostępność silnika',
    reestimating: 'Ponowne obliczanie pozostałego czasu …',
    unavailable: 'Pozostały czas nie jest jeszcze dostępny',
    finishing: 'Zapisywanie wyników analizy …',
    benchmark: 'Na podstawie zapisanego benchmarku',
    observed: 'Na podstawie zmierzonego tempa analizy',
  },
} satisfies Record<Locale, Record<Key, string>>;
export function remainingTimeText(
  locale: Locale,
  key: Key,
  values?: TranslationParams,
) {
  return formatMessage(locale, remainingTimeMessages[locale][key], values);
}

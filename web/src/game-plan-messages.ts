import { useI18n, type Locale } from './i18n';

const entries = {
  title: [
    'Game plan',
    'Spielplan',
    'Plan de juego',
    'Plan de jeu',
    'Plan gry',
    'Piano di gioco',
  ],
  white: ['White', 'Weiß', 'Blancas', 'Blancs', 'Białe', 'Bianchi'],
  black: ['Black', 'Schwarz', 'Negras', 'Noirs', 'Czarne', 'Neri'],
  opening: [
    'Opening',
    'Eröffnung',
    'Apertura',
    'Ouverture',
    'Otwarcie',
    'Apertura',
  ],
  pure_race: [
    'Pure race',
    'Reines Rennen',
    'Carrera pura',
    'Course pure',
    'Czysty wyścig',
    'Corsa pura',
  ],
  racing: [
    'Racing (contact)',
    'Rennspiel (mit Kontakt)',
    'Carrera (con contacto)',
    'Course (avec contact)',
    'Wyścig (z kontaktem)',
    'Corsa (con contatto)',
  ],
  attacking: [
    'Attacking / blitz',
    'Angriff / Blitz',
    'Ataque / blitz',
    'Attaque / blitz',
    'Atak / blitz',
    'Attacco / blitz',
  ],
  priming: [
    'Priming',
    'Prime-Spiel',
    'Construcción de prime',
    'Construction de prime',
    'Budowanie blokady',
    'Costruzione di prime',
  ],
  anchoring: [
    'Anchoring',
    'Ankerspiel',
    'Juego de anclas',
    'Jeu d’ancrage',
    'Gra z kotwicą',
    'Gioco di ancoraggio',
  ],
  finished: [
    'Game finished',
    'Spiel beendet',
    'Partida terminada',
    'Partie terminée',
    'Gra zakończona',
    'Partita terminata',
  ],
  loading: [
    'Loading game plan…',
    'Spielplan wird geladen…',
    'Cargando el plan de juego…',
    'Chargement du plan de jeu…',
    'Wczytywanie planu gry…',
    'Caricamento del piano di gioco…',
  ],
  unavailable: [
    'Game plan unavailable.',
    'Spielplan nicht verfügbar.',
    'Plan de juego no disponible.',
    'Plan de jeu indisponible.',
    'Plan gry niedostępny.',
    'Piano di gioco non disponibile.',
  ],
  retry: [
    'Retry',
    'Erneut versuchen',
    'Reintentar',
    'Réessayer',
    'Spróbuj ponownie',
    'Riprova',
  ],
  explanation: [
    'NeonGammon rules classify the displayed board structure. GNUbg provides move recommendations.',
    'NeonGammon-Regeln ordnen die Struktur der angezeigten Stellung ein. Zugempfehlungen kommen von GNUbg.',
    'Las reglas de NeonGammon clasifican la estructura de la posición mostrada. GNUbg proporciona las recomendaciones de jugadas.',
    'Les règles de NeonGammon classent la structure de la position affichée. GNUbg fournit les recommandations de coups.',
    'Reguły NeonGammon klasyfikują strukturę wyświetlonej pozycji. Rekomendacje ruchów pochodzą z GNUbg.',
    'Le regole di NeonGammon classificano la struttura della posizione visualizzata. GNUbg fornisce le raccomandazioni sulle mosse.',
  ],
} satisfies Record<string, [string, string, string, string, string, string]>;
type Key = keyof typeof entries;
const locales: Locale[] = ['en', 'de', 'es', 'fr', 'pl', 'it'];
export const gamePlanMessages = Object.fromEntries(
  locales.map((locale, i) => [
    locale,
    Object.fromEntries(
      Object.entries(entries).map(([key, values]) => [key, values[i]]),
    ),
  ]),
) as Record<Locale, Record<Key, string>>;

export function useGamePlanText() {
  const { locale } = useI18n();
  return (key: Key) => gamePlanMessages[locale][key];
}

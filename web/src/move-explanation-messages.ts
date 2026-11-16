import { useI18n, type Locale } from './i18n';
import { interpolate, type TranslationParams } from './i18n-core';
const entries = {
  title: [
    'What the plays do differently',
    'Was die Züge unterscheidet',
    'En qué se diferencian las jugadas',
    'Ce qui distingue les coups',
    'Czym różnią się zagrania',
    'In cosa differiscono le mosse',
  ],
  columns: [
    'Best play {best} · chosen play {played}',
    'Bester Zug {best} · gewählter Zug {played}',
    'Mejor jugada {best} · jugada elegida {played}',
    'Meilleur coup {best} · coup choisi {played}',
    'Najlepsze zagranie {best} · wybrane zagranie {played}',
    'Mossa migliore {best} · mossa scelta {played}',
  ],
  shots: [
    'Opponent rolls that hit a blot: {best} of 36 after the best play, {played} of 36 after the chosen play.',
    'Würfe des Gegners, die einen Einzelstein schlagen: {best} von 36 nach dem besten Zug, {played} von 36 nach dem gewählten Zug.',
    'Tiradas del rival que capturan una ficha suelta: {best} de 36 tras la mejor jugada, {played} de 36 tras la jugada elegida.',
    'Lancers adverses qui frappent un pion isolé : {best} sur 36 après le meilleur coup, {played} sur 36 après le coup choisi.',
    'Rzuty przeciwnika bijące pojedynczy pion: {best} z 36 po najlepszym zagraniu, {played} z 36 po wybranym zagraniu.',
    'Tiri dell’avversario che colpiscono una pedina isolata: {best} su 36 dopo la mossa migliore, {played} su 36 dopo la mossa scelta.',
  ],
  hits: [
    'Opponent checkers on the bar: {best} after the best play, {played} after the chosen play.',
    'Gegnerische Steine auf der Bar: {best} nach dem besten Zug, {played} nach dem gewählten Zug.',
    'Fichas rivales en la barra: {best} tras la mejor jugada, {played} tras la jugada elegida.',
    'Pions adverses sur la barre : {best} après le meilleur coup, {played} après le coup choisi.',
    'Piony przeciwnika na bandzie: {best} po najlepszym zagraniu, {played} po wybranym zagraniu.',
    'Pedine avversarie sul bar: {best} dopo la mossa migliore, {played} dopo la mossa scelta.',
  ],
  pointsBest: [
    'Only the best play holds these points: {points}.',
    'Nur der beste Zug hält diese Punkte: {points}.',
    'Solo la mejor jugada mantiene estos puntos: {points}.',
    'Seul le meilleur coup tient ces points : {points}.',
    'Tylko najlepsze zagranie utrzymuje te pola: {points}.',
    'Solo la mossa migliore tiene questi punti: {points}.',
  ],
  pointsPlayed: [
    'Only the chosen play holds these points: {points}.',
    'Nur der gewählte Zug hält diese Punkte: {points}.',
    'Solo la jugada elegida mantiene estos puntos: {points}.',
    'Seul le coup choisi tient ces points : {points}.',
    'Tylko wybrane zagranie utrzymuje te pola: {points}.',
    'Solo la mossa scelta tiene questi punti: {points}.',
  ],
  prime: [
    'Longest row of adjacent points: {best} after the best play, {played} after the chosen play.',
    'Längste Reihe benachbarter Punkte: {best} nach dem besten Zug, {played} nach dem gewählten Zug.',
    'Fila más larga de puntos contiguos: {best} tras la mejor jugada, {played} tras la jugada elegida.',
    'Plus longue suite de points adjacents : {best} après le meilleur coup, {played} après le coup choisi.',
    'Najdłuższy ciąg sąsiednich pól: {best} po najlepszym zagraniu, {played} po wybranym zagraniu.',
    'Fila più lunga di punti adiacenti: {best} dopo la mossa migliore, {played} dopo la mossa scelta.',
  ],
  blots: [
    'Blots left: {best} after the best play, {played} after the chosen play.',
    'Einzelsteine (Blots): {best} nach dem besten Zug, {played} nach dem gewählten Zug.',
    'Fichas sueltas: {best} tras la mejor jugada, {played} tras la jugada elegida.',
    'Pions isolés : {best} après le meilleur coup, {played} après le coup choisi.',
    'Pojedyncze piony: {best} po najlepszym zagraniu, {played} po wybranym zagraniu.',
    'Pedine isolate: {best} dopo la mossa migliore, {played} dopo la mossa scelta.',
  ],
  gammon: [
    'Chance to win a gammon: {best} after the best play, {played} after the chosen play.',
    'Chance auf einen Gammon-Gewinn: {best} nach dem besten Zug, {played} nach dem gewählten Zug.',
    'Probabilidad de ganar gammon: {best} tras la mejor jugada, {played} tras la jugada elegida.',
    'Chance de gagner un gammon : {best} après le meilleur coup, {played} après le coup choisi.',
    'Szansa na wygranie gammona: {best} po najlepszym zagraniu, {played} po wybranym zagraniu.',
    'Probabilità di vincere un gammon: {best} dopo la mossa migliore, {played} dopo la mossa scelta.',
  ],
  gammonLoss: [
    'Risk of losing a gammon: {best} after the best play, {played} after the chosen play.',
    'Risiko, einen Gammon zu verlieren: {best} nach dem besten Zug, {played} nach dem gewählten Zug.',
    'Riesgo de perder gammon: {best} tras la mejor jugada, {played} tras la jugada elegida.',
    'Risque de perdre un gammon : {best} après le meilleur coup, {played} après le coup choisi.',
    'Ryzyko przegrania gammona: {best} po najlepszym zagraniu, {played} po wybranym zagraniu.',
    'Rischio di perdere un gammon: {best} dopo la mossa migliore, {played} dopo la mossa scelta.',
  ],
  same: [
    'Both plays leave the same blots, shots and points. The difference lies in how the positions play out over the following rolls.',
    'Beide Züge lassen dieselben Einzelsteine, Treffer-Würfe und Punkte. Der Unterschied liegt darin, wie sich die Stellungen in den folgenden Würfen entwickeln.',
    'Ambas jugadas dejan las mismas fichas sueltas, tiradas de captura y puntos. La diferencia está en cómo evolucionan las posiciones en las tiradas siguientes.',
    'Les deux coups laissent les mêmes pions isolés, lancers frappants et points. La différence tient à l’évolution des positions sur les lancers suivants.',
    'Oba zagrania zostawiają te same pojedyncze piony, bijące rzuty i pola. Różnica wynika z tego, jak pozycje rozwijają się w kolejnych rzutach.',
    'Entrambe le mosse lasciano le stesse pedine isolate, gli stessi tiri che colpiscono e gli stessi punti. La differenza sta in come le posizioni si sviluppano nei tiri successivi.',
  ],
  note: [
    'Counted from the two resulting boards. The engine’s equity decides which play is better.',
    'Gezählt aus den beiden Folgestellungen. Welcher Zug besser ist, entscheidet die Equity der Engine.',
    'Contado a partir de las dos posiciones resultantes. La equity del motor decide qué jugada es mejor.',
    'Compté à partir des deux positions obtenues. L’équité du moteur décide quel coup est meilleur.',
    'Policzone z dwóch pozycji końcowych. O tym, które zagranie jest lepsze, decyduje equity silnika.',
    'Contato dalle due posizioni risultanti. L’equity del motore decide quale mossa è migliore.',
  ],
  glossary: ['Terms', 'Begriffe', 'Términos', 'Termes', 'Pojęcia', 'Termini'],
  equity: [
    'Equity: the average result of the game in points, from −3 to +3 before the cube, for the player on roll. Normalized cubeful equity counts the doubling cube and is scaled to the current cube value.',
    'Equity: das durchschnittliche Spielergebnis in Punkten für den Spieler am Zug, vor dem Cube von −3 bis +3. Die normierte Cubeful-Equity berücksichtigt den Dopplerwürfel und bezieht sich auf den aktuellen Cube-Wert.',
    'Equity: el resultado medio del juego en puntos para el jugador que tira, de −3 a +3 antes del cubo. La equity cubeful normalizada tiene en cuenta el cubo y se refiere a su valor actual.',
    'Équité : le résultat moyen de la partie en points pour le joueur au trait, de −3 à +3 avant le videau. L’équité cubeful normalisée tient compte du videau et se rapporte à sa valeur actuelle.',
    'Equity: średni wynik gry w punktach dla gracza na ruchu, od −3 do +3 przed kostką. Znormalizowana equity z kostką uwzględnia kostkę podwajającą i odnosi się do jej bieżącej wartości.',
    'Equity: il risultato medio della partita in punti per il giocatore di turno, da −3 a +3 prima del cubo. L’equity cubeful normalizzata tiene conto del cubo e si riferisce al suo valore attuale.',
  ],
  loss: [
    'Equity loss: best equity minus the equity of the chosen play. 0.020 is a small error, 0.080 a blunder with the default thresholds.',
    'Equity-Verlust: beste Equity minus Equity des gewählten Zugs. Mit den Standardschwellen ist 0,020 ein kleiner Fehler und 0,080 ein grober Fehler.',
    'Pérdida de equity: la mejor equity menos la de la jugada elegida. Con los umbrales por defecto, 0,020 es un error pequeño y 0,080 un error grave.',
    'Perte d’équité : meilleure équité moins celle du coup choisi. Avec les seuils par défaut, 0,020 est une petite erreur et 0,080 une grosse erreur.',
    'Strata equity: najlepsza equity minus equity wybranego zagrania. Przy domyślnych progach 0,020 to mały błąd, a 0,080 poważny błąd.',
    'Perdita di equity: l’equity migliore meno quella della mossa scelta. Con le soglie predefinite, 0,020 è un errore piccolo e 0,080 un errore grave.',
  ],
  pr: [
    'PR (performance rating): average equity loss per decision × 500. Lower is better; about 5 is expert level and about 10 a strong club player.',
    'PR (Performance Rating): durchschnittlicher Equity-Verlust pro Entscheidung × 500. Niedriger ist besser; etwa 5 entspricht Expertenniveau, etwa 10 einem starken Clubspieler.',
    'PR (performance rating): pérdida media de equity por decisión × 500. Cuanto más bajo, mejor; unos 5 son nivel experto y unos 10 un buen jugador de club.',
    'PR (performance rating) : perte moyenne d’équité par décision × 500. Plus c’est bas, mieux c’est ; environ 5 correspond à un expert et environ 10 à un bon joueur de club.',
    'PR (performance rating): średnia strata equity na decyzję × 500. Im niżej, tym lepiej; około 5 to poziom eksperta, około 10 to mocny gracz klubowy.',
    'PR (performance rating): perdita media di equity per decisione × 500. Più basso è meglio; circa 5 è livello esperto e circa 10 un forte giocatore di circolo.',
  ],
  ply: [
    'Ply: how many rolls the engine looks ahead. 0-ply is the neural network’s direct estimate; 2-ply averages over the opponent’s and the player’s next roll.',
    'Ply: wie viele Würfe die Engine vorausrechnet. 0-ply ist die direkte Schätzung des neuronalen Netzes; 2-ply mittelt über den nächsten Wurf des Gegners und den eigenen.',
    'Ply: cuántas tiradas calcula el motor por adelantado. 0-ply es la estimación directa de la red neuronal; 2-ply promedia la siguiente tirada del rival y la propia.',
    'Ply : nombre de lancers anticipés par le moteur. 0-ply est l’estimation directe du réseau de neurones ; 2-ply fait la moyenne sur le prochain lancer de l’adversaire puis du joueur.',
    'Ply: ile rzutów naprzód liczy silnik. 0-ply to bezpośrednia ocena sieci neuronowej; 2-ply uśrednia następny rzut przeciwnika i własny.',
    'Ply: quanti tiri in avanti calcola il motore. 0-ply è la stima diretta della rete neurale; 2-ply fa la media sul prossimo tiro dell’avversario e sul proprio.',
  ],
} satisfies Record<string, [string, string, string, string, string, string]>;
type Key = keyof typeof entries;
const locales: Locale[] = ['en', 'de', 'es', 'fr', 'pl', 'it'];
export const moveExplanationMessages = Object.fromEntries(
  locales.map((locale, i) => [
    locale,
    Object.fromEntries(
      Object.entries(entries).map(([key, values]) => [key, values[i]]),
    ),
  ]),
) as Record<Locale, Record<Key, string>>;
export function useMoveExplanationText() {
  const { locale } = useI18n();
  return (key: Key, params: TranslationParams = {}) =>
    interpolate(moveExplanationMessages[locale][key], params);
}

import { expect, it } from 'vitest';
import { groupDecks } from './deck-groups';

const decks = [
  { id: '1', name: 'Training' },
  { id: '2', name: 'Rainer::UBC::Final 2021::Schiemann' },
  { id: '3', name: 'Rainer::UBC::Final 2021::Mochy' },
  { id: '4', name: 'Mochy::Backgame' },
];

it('groups decks by parent path and keeps the last level as the label', () => {
  expect(
    groupDecks(decks).map((group) => [
      group.path,
      group.decks.map((entry) => entry.label),
    ]),
  ).toEqual([
    ['', ['Training']],
    ['Rainer › UBC › Final 2021', ['Schiemann', 'Mochy']],
    ['Mochy', ['Backgame']],
  ]);
});

it('filters by any part of the name and keeps the selected deck', () => {
  const groups = groupDecks(decks, 'mochy', '1');
  expect(groups.flatMap((group) => group.decks.map((d) => d.deck.id))).toEqual([
    '1',
    '3',
    '4',
  ]);
});

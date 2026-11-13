import type { Ref } from 'react';
import { useMistakeText } from './recurring-mistakes-messages';
import { SearchFilterSelect } from './SearchFilterForm';

export default function MistakePlayerSelect({
  players,
  value,
  onChange,
  selectRef,
}: {
  players: string[];
  value: string;
  onChange: (player: string) => void;
  selectRef?: Ref<HTMLSelectElement>;
}) {
  const t = useMistakeText();
  return (
    <SearchFilterSelect
      label={t('player')}
      value={value}
      onChange={onChange}
      selectRef={selectRef}
    >
      <option value="">{t('choose')}</option>
      {players.map((name) => (
        <option key={name} value={name}>
          {name}
        </option>
      ))}
    </SearchFilterSelect>
  );
}

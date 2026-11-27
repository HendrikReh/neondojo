import { languages, type Locale } from '../shared/i18n/i18n';
import type { AppSettings } from './settings-api';
import { useSettingsText } from './settings-messages';
import {
  MyPlayerProfileField,
  type useMyPlayerProfileDraft,
} from './MyPlayerProfileSetting';

export default function GeneralSettingsSection({
  id,
  disabled,
  hidden,
  applyLabel,
  draftLocale,
  onLocale,
  playerName,
  onPlayerName,
  myProfile,
}: {
  id: string;
  disabled: boolean;
  hidden: boolean;
  applyLabel: string;
  draftLocale: Locale;
  onLocale: (locale: Locale) => void;
  playerName: AppSettings['player_name'];
  onPlayerName: (name: string) => void;
  myProfile: ReturnType<typeof useMyPlayerProfileDraft>;
}) {
  const t = useSettingsText();

  return (
    <fieldset className="settings-section" disabled={disabled} hidden={hidden}>
      <legend>{t('general')}</legend>
      <label className="settings-field" htmlFor={`${id}-language`}>
        <span>{t('language')}</span>
        <select
          id={`${id}-language`}
          aria-label={t('language')}
          value={draftLocale}
          onChange={(event) => onLocale(event.target.value as Locale)}
          aria-describedby={`${id}-language-help`}
        >
          {languages.map((language) => (
            <option key={language.id} value={language.id} lang={language.id}>
              {language.name}
            </option>
          ))}
        </select>
      </label>
      <p id={`${id}-language-help`} className="settings-help">
        {t('languageHelp', { action: applyLabel })}
      </p>
      <label className="settings-field" htmlFor={`${id}-player`}>
        <span>{t('playerName')}</span>
        <input
          id={`${id}-player`}
          value={playerName}
          maxLength={120}
          onChange={(event) => onPlayerName(event.target.value)}
          aria-describedby={`${id}-player-help`}
          autoComplete="nickname"
        />
      </label>
      <p id={`${id}-player-help`} className="settings-help">
        {t('playerHelp')}
      </p>
      <MyPlayerProfileField id={`${id}-my-profile`} state={myProfile} />
    </fieldset>
  );
}

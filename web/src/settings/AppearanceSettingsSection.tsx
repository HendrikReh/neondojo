import {
  uiThemes,
  type UiAppearance,
  type UiMode,
  type UiTheme,
} from './appearance/ui-appearance';
import { useUiAppearanceText } from './appearance/ui-appearance-messages';
import UiAppearancePreview from './appearance/UiAppearancePreview';

export default function AppearanceSettingsSection({
  id,
  disabled,
  hidden,
  applyLabel,
  appearance,
  onChange,
}: {
  id: string;
  disabled: boolean;
  hidden: boolean;
  applyLabel: string;
  appearance: UiAppearance;
  onChange: (updater: (previous: UiAppearance) => UiAppearance) => void;
}) {
  const appearanceT = useUiAppearanceText();

  return (
    <fieldset className="settings-section" disabled={disabled} hidden={hidden}>
      <legend>{appearanceT('appearance')}</legend>
      <p className="settings-help settings-intro">
        {appearanceT('help', { action: applyLabel })}
      </p>
      <div className="settings-preview-layout">
        <div>
          <label className="settings-field" htmlFor={`${id}-interface-theme`}>
            <span>{appearanceT('interfaceTheme')}</span>
            <select
              id={`${id}-interface-theme`}
              value={appearance.theme}
              onChange={(event) =>
                onChange((previous) => ({
                  ...previous,
                  theme: event.target.value as UiTheme,
                }))
              }
            >
              {uiThemes.map((name) => (
                <option key={name} value={name}>
                  {appearanceT(name)}
                </option>
              ))}
            </select>
          </label>
          <label className="settings-field" htmlFor={`${id}-display-mode`}>
            <span>{appearanceT('displayMode')}</span>
            <select
              id={`${id}-display-mode`}
              value={appearance.mode}
              aria-describedby={`${id}-system-help`}
              onChange={(event) =>
                onChange((previous) => ({
                  ...previous,
                  mode: event.target.value as UiMode,
                }))
              }
            >
              {(['light', 'dark', 'system'] as const).map((mode) => (
                <option key={mode} value={mode}>
                  {appearanceT(mode)}
                </option>
              ))}
            </select>
          </label>
          <p className="settings-help" id={`${id}-system-help`}>
            {appearanceT('systemHelp')}
          </p>
        </div>
        <UiAppearancePreview appearance={appearance} />
      </div>
    </fieldset>
  );
}

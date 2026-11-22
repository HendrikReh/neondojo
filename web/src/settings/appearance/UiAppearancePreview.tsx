import type { UiAppearance } from './ui-appearance';
import { useResolvedUiMode } from './useUiAppearance';
import { useUiAppearanceText } from './ui-appearance-messages';

export default function UiAppearancePreview({
  appearance,
}: {
  appearance: UiAppearance;
}) {
  const mode = useResolvedUiMode(appearance.mode);
  const t = useUiAppearanceText();
  return (
    <figure
      className="ui-appearance-preview"
      data-ui-theme={appearance.theme}
      data-ui-mode={mode}
      aria-label={t('preview')}
    >
      <figcaption>{t('preview')}</figcaption>
      <div className="ui-preview-header">
        <strong>NeonDojo</strong>
        <span className="ui-preview-tab">{t('analysis')}</span>
        <span>{t('database')}</span>
      </div>
      <div className="ui-preview-content">
        <div className="ui-preview-heading">
          <strong>{t('position')}</strong>
          <span>
            {t(appearance.theme)} · {t(mode)}
          </span>
        </div>
        <div className="ui-preview-card">
          <strong>{t('candidates')}</strong>
          <table>
            <thead>
              <tr>
                <th>{t('move')}</th>
                <th>{t('equity')}</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  13/8 6/5{' '}
                  <span className="ui-preview-success">{t('best')}</span>
                </td>
                <td>+0.214</td>
              </tr>
              <tr>
                <td>
                  24/18{' '}
                  <span className="ui-preview-warning">{t('warning')}</span>
                </td>
                <td>+0.172</td>
              </tr>
              <tr>
                <td>
                  13/7 <span className="ui-preview-danger">{t('error')}</span>
                </td>
                <td>−0.021</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </figure>
  );
}

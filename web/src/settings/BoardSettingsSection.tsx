import { useRef, useLayoutEffect } from 'react';
import type { Position } from '../shared/http/api';
import Board from '../shared/board/Board';
import { useArchiveThemes, builtInThemes } from '../shared/board/themes';
import type { BoardArtworkSelection } from './configuration/configuration-settings';
import { useProfileText } from './profiles/profile-messages';
import { useSettingsText } from './settings-messages';

// A fixed, legal GNUbg position with checkers on the bar and borne off on both
// sides. It belongs only to the preview; it never enters the match library.
const previewPosition: Position = {
  id: '00000000-0000-0000-0000-000000000001',
  match_uuid: '00000000-0000-0000-0000-000000000002',
  board: [
    -1, -1, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -4, 4, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    1, 1,
  ],
  dice: [3, 2],
  player: 0,
  game_no: 1,
  ordinal: 0,
  ply: 0,
  position_id: '4HPwACjwOXgAFA',
  gnubg_match_id: 'EYHpACAAGAAE',
  notation: '',
};

export default function BoardSettingsSection({
  disabled,
  artwork,
  onChange,
}: {
  disabled: boolean;
  artwork: BoardArtworkSelection;
  onChange: (artwork: BoardArtworkSelection) => void;
}) {
  const t = useSettingsText();
  const profileT = useProfileText();
  const { theme, checkerTheme, cubeTheme } = artwork;
  const archiveThemes = useArchiveThemes(true);
  const themes = [...builtInThemes, ...archiveThemes];
  const boardGallery = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const gallery = boardGallery.current;
    if (!gallery) return;
    const selected = gallery.querySelector('button[aria-pressed="true"]');
    if (!selected) return;

    const item = selected.getBoundingClientRect();
    const galleryBox = gallery.getBoundingClientRect();
    const padding = getComputedStyle(gallery);
    const top = galleryBox.top + parseFloat(padding.paddingTop);
    const bottom = galleryBox.bottom - parseFloat(padding.paddingBottom);
    if (item.top < top) gallery.scrollTop += Math.floor(item.top - top);
    else if (item.bottom > bottom)
      gallery.scrollTop += Math.ceil(item.bottom - bottom);
  }, []);
  const selectedName = themes.find(({ id }) => id === theme)?.name ?? theme;
  return (
    <fieldset
      className="settings-section settings-board-section"
      disabled={disabled}
    >
      <legend>{t('board')}</legend>
      <p className="settings-help settings-intro">{t('boardHelp')}</p>
      <div className="settings-preview-layout">
        <div
          ref={boardGallery}
          className="settings-theme-gallery"
          role="group"
          tabIndex={0}
          aria-label={t('gallery')}
        >
          {themes.map((entry) => {
            const archive = archiveThemes.find(({ id }) => id === entry.id);
            return (
              <button
                key={entry.id}
                type="button"
                className="settings-theme-choice"
                aria-label={entry.name}
                aria-pressed={theme === entry.id}
                onClick={() => onChange({ ...artwork, theme: entry.id })}
              >
                <span className="settings-theme-thumbnail" aria-hidden="true">
                  {archive ? (
                    <>
                      <img src={archive.background.src} alt="" loading="lazy" />
                      <img
                        className="settings-thumb-checker settings-thumb-light"
                        src={archive.checkers.white[0].src}
                        alt=""
                        loading="lazy"
                      />
                      <img
                        className="settings-thumb-checker settings-thumb-dark"
                        src={archive.checkers.black[0].src}
                        alt=""
                        loading="lazy"
                      />
                    </>
                  ) : (
                    <Board
                      position={previewPosition}
                      theme={entry.id}
                      checkerTheme={null}
                      cubeTheme={null}
                      white=""
                      black=""
                    />
                  )}
                  {theme === entry.id && (
                    <span className="settings-selected">✓ {t('selected')}</span>
                  )}
                </span>
                <span className="settings-theme-name">{entry.name}</span>
              </button>
            );
          })}
        </div>
        <section
          className="settings-preview"
          aria-label={t('preview', { theme: selectedName })}
        >
          <h3>{t('preview', { theme: selectedName })}</h3>
          <div className="settings-field-pair">
            {(
              [
                ['checkerTheme', profileT('checker_theme')],
                ['cubeTheme', profileT('cube_theme')],
              ] as const
            ).map(([field, label]) => {
              const value = artwork[field];
              return (
                <label key={field} className="settings-field">
                  {label}
                  <select
                    value={value ?? ''}
                    onChange={(event) =>
                      onChange({
                        ...artwork,
                        [field]: event.target.value || null,
                      })
                    }
                  >
                    <option value="">{t('followBoard')}</option>
                    {value && !themes.some(({ id }) => id === value) && (
                      <option value={value} disabled>
                        {value} ({t('unavailable')})
                      </option>
                    )}
                    {themes.map((entry) => (
                      <option key={entry.id} value={entry.id}>
                        {entry.name}
                      </option>
                    ))}
                  </select>
                </label>
              );
            })}
          </div>
          <Board
            position={previewPosition}
            theme={theme}
            checkerTheme={checkerTheme}
            cubeTheme={cubeTheme}
            white={t('light')}
            black={t('dark')}
          />
        </section>
      </div>
    </fieldset>
  );
}

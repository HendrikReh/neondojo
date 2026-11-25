import { useRef, useLayoutEffect } from 'react';
import type { Position } from '../shared/http/api';
import Board from '../shared/board/Board';
import {
  useArchiveThemes,
  builtInThemes,
  type Theme,
} from '../shared/board/themes';
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

/** The Board category: the theme gallery and a preview of the chosen theme. */
export default function BoardSettingsSection({
  disabled,
  theme,
  onTheme,
}: {
  disabled: boolean;
  theme: Theme;
  onTheme: (theme: Theme) => void;
}) {
  const t = useSettingsText();
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
  const selectedTheme = themes.find(({ id }) => id === theme) ?? themes[0];
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
                onClick={() => onTheme(entry.id)}
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
          aria-label={t('preview', { theme: selectedTheme.name })}
        >
          <h3>{t('preview', { theme: selectedTheme.name })}</h3>
          <Board
            position={previewPosition}
            theme={theme}
            white={t('light')}
            black={t('dark')}
          />
        </section>
      </div>
    </fieldset>
  );
}

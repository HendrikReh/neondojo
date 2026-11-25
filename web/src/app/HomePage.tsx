import { useEffect, useState } from 'react';
import Board from '../shared/board/Board';
import type { Theme } from '../shared/board/themes';
import type { Match, Position } from '../shared/http/api';
import { useI18n } from '../shared/i18n/i18n';
import { AppError } from '../shared/i18n/i18n-core';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import ActionButton from '../shared/ui/ActionButton';
import MatchDropZone from '../workspaces/library/import/MatchDropZone';
import { loadPlaySession, type PlaySession } from '../workspaces/play/play-api';
import { storedSession } from '../workspaces/play/usePlaySession';
import { useLearningSummary } from '../workspaces/study/useLearningSummary';
import type { MatchImporter } from './useMatchImporter';
import './home.css';

type Props = {
  match: Match | null;
  position?: Position;
  theme: Theme;
  loading: boolean;
  importer: MatchImporter;
  restoredPlayId?: string;
  onContinue: () => void;
  onOpenLibrary: () => void;
  onPlay: () => void;
  onResumePlay: (session: PlaySession) => void;
  onSearch: () => void;
  onStudy: (id: string) => void;
};

export default function HomePage({
  match,
  position,
  theme,
  loading,
  importer,
  restoredPlayId,
  onContinue,
  onOpenLibrary,
  onPlay,
  onResumePlay,
  onSearch,
  onStudy,
}: Props) {
  const w = useWorkspaceText();
  const { t, formatError } = useI18n();
  const { summary } = useLearningSummary(true);
  const deck = summary.status === 'ready' && summary.data.next_deck;
  const sessionId = restoredPlayId ?? storedSession();
  const [play, setPlay] = useState<PlaySession>();
  const [playError, setPlayError] = useState<unknown>();
  useEffect(() => {
    setPlay(undefined);
    setPlayError(undefined);
    if (!sessionId) return;
    const abort = new AbortController();
    void loadPlaySession(sessionId, abort.signal)
      .then((session) => {
        if (
          !abort.signal.aborted &&
          !session.money_session_ended &&
          (session.phase !== 'match_over' || session.match_length === 0)
        )
          setPlay(session);
      })
      .catch((error: unknown) => {
        if (
          !abort.signal.aborted &&
          !(error instanceof AppError && String(error.params.status) === '404')
        )
          setPlayError(error);
      });
    return () => abort.abort();
  }, [sessionId]);
  const importLabel = t(
    importer.uploading ? 'app.importRunning' : 'app.importPlain',
  );
  const importAction = (primary: boolean) => (
    <ActionButton
      icon="import"
      className={primary ? 'primary' : undefined}
      label={importLabel}
      disabled={importer.uploading}
      onClick={() => importer.chooseFiles()}
    />
  );
  return (
    <section className="home-page" aria-labelledby="home-heading">
      <h1 id="home-heading">{w('homeTitle')}</h1>
      <div className="home-columns">
        <section className="home-feature">
          <h2>{w(match || loading ? 'continueTitle' : 'firstImport')}</h2>
          {loading ? (
            <p role="status">{w('lastMatch')}</p>
          ) : match ? (
            <>
              <p className="home-match">
                {match.white} · {match.black}
              </p>
              {position && (
                <div className="home-board">
                  <Board
                    position={position}
                    theme={theme}
                    white={match.white}
                    black={match.black}
                  />
                </div>
              )}
              <ActionButton
                className="primary"
                icon="resume"
                onClick={onContinue}
                label={
                  position
                    ? w('resumeMove', {
                        game: position.game_no,
                        move: position.ply,
                      })
                    : w('resumeAnalysis')
                }
              />
            </>
          ) : (
            <div className="welcome-actions">
              {importAction(true)}
              <ActionButton
                icon="open"
                label={w('openMatch')}
                onClick={onOpenLibrary}
              />
              <MatchDropZone
                disabled={importer.uploading}
                onDrop={importer.drop}
              />
            </div>
          )}
        </section>
        <section className="home-shortcuts">
          <h2>{w('quickStart')}</h2>
          {(match || loading) && importAction(false)}
          <ActionButton
            icon="dice"
            label={w('playComputer')}
            onClick={onPlay}
          />
          <ActionButton
            icon="search"
            label={w('findPosition')}
            onClick={onSearch}
          />
          {play && play.id === sessionId && (
            <ActionButton
              icon="resume"
              label={w('resumePlay')}
              onClick={() => onResumePlay(play)}
            />
          )}
          {deck && summary.status === 'ready' && (
            <ActionButton
              icon="cards"
              label={w('cardsToday', {
                count: summary.data.today_count ?? summary.data.due_count,
              })}
              onClick={() => onStudy(deck.id)}
            />
          )}
          {summary.status === 'error' && (
            <p role="status">{formatError(summary.error)}</p>
          )}
          {!!playError && <p role="status">{formatError(playError)}</p>}
        </section>
      </div>
    </section>
  );
}

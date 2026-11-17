import {
  useEffect,
  useRef,
  useState,
  type ComponentProps,
  type KeyboardEvent,
} from 'react';
import LearningWorkspace from './LearningPanel';
import StudyOverview from './StudyOverview';
import StudyReports from './StudyReports';
import type { AssessmentEntry } from './match-assessment-api';
import RecurringMistakes from './RecurringMistakes';
import SimilarPracticeHistory from './SimilarPracticeHistory';
import MistakePracticeHistory from './MistakePracticeHistory';
import { usePositionText } from './position-browser-messages';
import { useStudyOverviewText, useStudyText } from './study-messages';
import './study.css';
type Area = 'overview' | 'cards' | 'reports' | 'mistakes' | 'practice';
const areas: Area[] = ['overview', 'cards', 'reports', 'mistakes', 'practice'];
export default function StudyWorkspace({
  onStudyDeck,
  onOpenPlayers,
  requestedAssessment,
  ...props
}: ComponentProps<typeof LearningWorkspace> & {
  onStudyDeck: (id: string) => void;
  onOpenPlayers: () => void;
  requestedAssessment?: AssessmentEntry;
}) {
  const t = useStudyText();
  const overviewText = useStudyOverviewText();
  const p = usePositionText();
  const [area, setArea] = useState<Area>('overview');
  // The overview hands its player to the full list; the token makes a repeated
  // request for the same player count again.
  const [mistakePlayer, setMistakePlayer] = useState<{
    name: string;
    token: number;
  }>();
  // The player the overview shows, offered to the list when it has none.
  const [overviewPlayer, setOverviewPlayer] = useState<string>();
  const tabs = useRef<Partial<Record<Area, HTMLButtonElement | null>>>({});
  const token = props.requestedDeck?.token;
  useEffect(() => {
    if (token !== undefined) setArea('cards');
  }, [token]);
  useEffect(() => {
    if (requestedAssessment) setArea('reports');
  }, [requestedAssessment]);
  function openArea(next: Area) {
    setArea(next);
    tabs.current[next]?.focus();
  }
  function navigate(event: KeyboardEvent<HTMLButtonElement>, current: Area) {
    let next: Area;
    if (event.key === 'Home') next = areas[0];
    else if (event.key === 'End') next = areas[areas.length - 1];
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight')
      next =
        areas[
          (areas.indexOf(current) +
            (event.key === 'ArrowRight' ? 1 : -1) +
            areas.length) %
            areas.length
        ];
    else return;
    event.preventDefault();
    openArea(next);
  }
  const visible = props.visible ?? true;
  return (
    <div className="study-workspace">
      <nav
        className="study-tabs text-navigation"
        role="tablist"
        aria-label={t('navigation')}
      >
        {areas.map((item) => (
          <button
            key={item}
            type="button"
            role="tab"
            id={`study-tab-${item}`}
            aria-controls={`study-panel-${item}`}
            aria-selected={area === item}
            tabIndex={area === item ? 0 : -1}
            ref={(el) => {
              tabs.current[item] = el;
            }}
            onClick={() => setArea(item)}
            onKeyDown={(event) => navigate(event, item)}
          >
            {item === 'overview'
              ? overviewText('tab')
              : item === 'practice'
                ? p('practiceHistory')
                : t(item)}
          </button>
        ))}
      </nav>
      <div
        role="tabpanel"
        id="study-panel-overview"
        aria-labelledby="study-tab-overview"
        hidden={area !== 'overview'}
      >
        <StudyOverview
          visible={visible && area === 'overview'}
          revision={props.revision}
          theme={props.theme}
          onOpenCards={() => openArea('cards')}
          onOpenPlayers={onOpenPlayers}
          onPlayer={setOverviewPlayer}
          onOpenMistakes={(name) => {
            if (name)
              setMistakePlayer((current) => ({
                name,
                token: (current?.token ?? 0) + 1,
              }));
            openArea('mistakes');
          }}
          onStudyDeck={(id) => {
            openArea('cards');
            onStudyDeck(id);
          }}
        />
      </div>
      <div
        role="tabpanel"
        id="study-panel-cards"
        aria-labelledby="study-tab-cards"
        hidden={area !== 'cards'}
      >
        <LearningWorkspace {...props} visible={visible && area === 'cards'} />
      </div>
      <div
        role="tabpanel"
        id="study-panel-reports"
        aria-labelledby="study-tab-reports"
        hidden={area !== 'reports'}
      >
        <StudyReports
          visible={visible && area === 'reports'}
          theme={props.theme}
          requestedAssessment={requestedAssessment}
        />
      </div>
      <div
        role="tabpanel"
        id="study-panel-mistakes"
        aria-labelledby="study-tab-mistakes"
        hidden={area !== 'mistakes'}
      >
        <RecurringMistakes
          visible={visible && area === 'mistakes'}
          theme={props.theme}
          requestedPlayer={mistakePlayer}
          suggestedPlayer={overviewPlayer}
          onOpenPlayers={onOpenPlayers}
        />
      </div>
      <div
        role="tabpanel"
        id="study-panel-practice"
        aria-labelledby="study-tab-practice"
        hidden={area !== 'practice'}
      >
        {visible && area === 'practice' && (
          <>
            <h1 className="study-practice-heading">{p('practiceHistory')}</h1>
            <MistakePracticeHistory />
            <SimilarPracticeHistory />
          </>
        )}
      </div>
    </div>
  );
}

import { useWorkspaceSession } from '../../shared/hooks/useWorkspaceSession';
import { TabList, useRovingTabs } from '../../shared/ui/TabList';
import { useEffect, useState, type ComponentProps } from 'react';
import LearningWorkspace from './flashcards/LearningPanel';
import StudyOverview from './StudyOverview';
import StudyReports from './reports/StudyReports';
import type { AssessmentEntry } from './reports/match-assessment-api';
import RecurringMistakes from './mistakes/RecurringMistakes';
import SimilarPracticeHistory from './practice/SimilarPracticeHistory';
import MistakePracticeHistory from './mistakes/MistakePracticeHistory';
import { usePositionText } from '../positions/position-browser-messages';
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
  // Whether mistake statistics leave out matches played with learning aids;
  // the overview and the list share and remember the choice.
  const [independentOnly, setIndependentOnly] = useWorkspaceSession(
    'study.mistakes.independentOnly',
    undefined,
    false,
    (stored) => (typeof stored === 'boolean' ? stored : undefined),
    'layout',
  );
  // The player the overview shows, offered to the list when it has none.
  const [overviewPlayer, setOverviewPlayer] = useState<string>();
  const roving = useRovingTabs<Area>({
    items: areas,
    active: area,
    onSelect: setArea,
  });
  const token = props.requestedDeck?.token;
  useEffect(() => {
    if (token !== undefined) setArea('cards');
  }, [token]);
  useEffect(() => {
    if (requestedAssessment) setArea('reports');
  }, [requestedAssessment]);
  function openArea(next: Area) {
    setArea(next);
    roving.focusTab(next);
  }
  const visible = props.visible ?? true;
  return (
    <div className="study-workspace">
      <TabList
        as="nav"
        className="study-tabs text-navigation"
        ariaLabel={t('navigation')}
        active={area}
        onSelect={setArea}
        roving={roving}
        tabs={areas.map((item) => ({
          value: item,
          id: `study-tab-${item}`,
          controls: `study-panel-${item}`,
          label:
            item === 'overview'
              ? overviewText('tab')
              : item === 'practice'
                ? p('practiceHistory')
                : t(item),
        }))}
      />
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
          independentOnly={independentOnly}
          onIndependentOnly={setIndependentOnly}
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
          independentOnly={independentOnly}
          onIndependentOnly={setIndependentOnly}
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

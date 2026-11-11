import {
  useEffect,
  useRef,
  useState,
  type ComponentProps,
  type KeyboardEvent,
} from 'react';
import LearningWorkspace from './LearningPanel';
import StudyReports from './StudyReports';
import SimilarPracticeHistory from './SimilarPracticeHistory';
import { usePositionText } from './position-browser-messages';
import { useStudyText } from './study-messages';
import './study.css';
type Area = 'cards' | 'reports' | 'practice';
const areas: Area[] = ['cards', 'reports', 'practice'];
export default function StudyWorkspace(
  props: ComponentProps<typeof LearningWorkspace>,
) {
  const t = useStudyText();
  const p = usePositionText();
  const [area, setArea] = useState<Area>('cards');
  const tabs = useRef<Partial<Record<Area, HTMLButtonElement | null>>>({});
  const token = props.requestedDeck?.token;
  useEffect(() => {
    if (token !== undefined) setArea('cards');
  }, [token]);
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
    setArea(next);
    tabs.current[next]?.focus();
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
            {item === 'practice' ? p('practiceHistory') : t(item)}
          </button>
        ))}
      </nav>
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
        />
      </div>
      <div
        role="tabpanel"
        id="study-panel-practice"
        aria-labelledby="study-tab-practice"
        hidden={area !== 'practice'}
      >
        {visible && area === 'practice' && <SimilarPracticeHistory />}
      </div>
    </div>
  );
}

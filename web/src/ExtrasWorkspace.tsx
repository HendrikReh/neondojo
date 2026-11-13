import { useRef, type KeyboardEvent } from 'react';
import BenchmarkWorkspace from './BenchmarkWorkspace';
import { useBenchmarkText } from './benchmark-messages';
import DatabaseBackupWorkspace from './DatabaseBackupWorkspace';
import { databaseBackupMessages } from './database-backup-messages';
import EngineMatchWorkspace from './EngineMatchWorkspace';
import { engineMatchText } from './engine-match-messages';
import { useI18n } from './i18n';
import VersionInfo from './VersionInfo';

const tools = ['benchmark', 'matches', 'backup', 'versions'] as const;
export type ExtrasTool = (typeof tools)[number];
const storageKey = 'bgtutor.extrasTool';

export function readExtrasTool(): ExtrasTool {
  try {
    const stored = localStorage.getItem(storageKey);
    return tools.find((tool) => tool === stored) ?? 'benchmark';
  } catch {
    return 'benchmark';
  }
}

export function saveExtrasTool(tool: ExtrasTool) {
  try {
    localStorage.setItem(storageKey, tool);
  } catch {
    // Extras navigation also works without persistent storage.
  }
}

const versionTabs = {
  en: 'Versions',
  de: 'Versionen',
  es: 'Versiones',
  fr: 'Versions',
  it: 'Versioni',
  pl: 'Wersje',
};

export default function ExtrasWorkspace({
  visible,
  defaultPlies,
  playPlies,
  tool,
  setTool,
}: {
  visible: boolean;
  defaultPlies: number;
  playPlies: number | undefined;
  tool: ExtrasTool;
  setTool: (tool: ExtrasTool) => void;
}) {
  const benchmark = useBenchmarkText();
  const { locale } = useI18n();
  const tabs = useRef<Partial<Record<ExtrasTool, HTMLButtonElement | null>>>(
    {},
  );
  const labels = {
    benchmark: benchmark('benchmark'),
    matches: engineMatchText(locale, 'title'),
    backup: databaseBackupMessages[locale].title,
    versions: versionTabs[locale],
  };
  const panels = {
    benchmark: (
      <BenchmarkWorkspace
        visible={visible && tool === 'benchmark'}
        defaultPlies={defaultPlies}
        playPlies={playPlies}
      />
    ),
    matches: <EngineMatchWorkspace visible={visible && tool === 'matches'} />,
    backup: <DatabaseBackupWorkspace />,
    versions: visible && tool === 'versions' && <VersionInfo />,
  };

  function navigate(
    event: KeyboardEvent<HTMLButtonElement>,
    current: ExtrasTool,
  ) {
    const index = tools.indexOf(current);
    let next: number;
    if (event.key === 'ArrowRight') next = (index + 1) % tools.length;
    else if (event.key === 'ArrowLeft')
      next = (index + tools.length - 1) % tools.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = tools.length - 1;
    else return;
    event.preventDefault();
    setTool(tools[next]);
    tabs.current[tools[next]]?.focus();
  }

  return (
    <>
      <nav
        className="text-navigation extras-navigation"
        role="tablist"
        aria-label={benchmark('navigation')}
      >
        {tools.map((item) => (
          <button
            key={item}
            type="button"
            role="tab"
            id={`extras-tab-${item}`}
            aria-controls={`extras-panel-${item}`}
            aria-selected={tool === item}
            tabIndex={tool === item ? 0 : -1}
            ref={(element) => {
              tabs.current[item] = element;
            }}
            onClick={() => setTool(item)}
            onKeyDown={(event) => navigate(event, item)}
          >
            {labels[item]}
          </button>
        ))}
      </nav>
      {tools.map((item) => (
        <div
          key={item}
          role="tabpanel"
          id={`extras-panel-${item}`}
          aria-labelledby={`extras-tab-${item}`}
          hidden={tool !== item}
        >
          {panels[item]}
        </div>
      ))}
    </>
  );
}

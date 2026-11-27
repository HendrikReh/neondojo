import type { Match } from '../../shared/http/api';
import { TabList } from '../../shared/ui/TabList';
import BenchmarkWorkspace from './BenchmarkWorkspace';
import { useBenchmarkText } from './benchmark-messages';
import DatabaseBackupWorkspace from './DatabaseBackupWorkspace';
import { databaseBackupMessages } from './database-backup-messages';
import EngineMatchWorkspace from './EngineMatchWorkspace';
import { engineMatchText } from './engine-match-messages';
import { useI18n } from '../../shared/i18n/i18n';
import VersionInfo from './VersionInfo';
import { versionInfoText } from './version-info-messages';
import { extrasTools as tools, type ExtrasTool } from './extras-tool';

export default function ExtrasWorkspace({
  visible,
  defaultPlies,
  playPlies,
  tool,
  setTool,
  onOpenMatch,
  benchmarkActive,
  onBenchmarkActivity,
}: {
  visible: boolean;
  defaultPlies: number;
  playPlies: number | undefined;
  tool: ExtrasTool;
  setTool: (tool: ExtrasTool) => void;
  onOpenMatch: (match: Match) => void;
  /** Whether the global status knows of a queued or running benchmark. */
  benchmarkActive?: boolean;
  onBenchmarkActivity?: (active: boolean) => void;
}) {
  const benchmark = useBenchmarkText();
  const { locale } = useI18n();
  const labels = {
    benchmark: benchmark('benchmark'),
    matches: engineMatchText(locale, 'title'),
    backup: databaseBackupMessages[locale].title,
    versions: versionInfoText(locale, 'tab'),
  };
  const panels = {
    benchmark: (
      <BenchmarkWorkspace
        visible={visible && tool === 'benchmark'}
        defaultPlies={defaultPlies}
        playPlies={playPlies}
        activeElsewhere={benchmarkActive}
        onActivity={onBenchmarkActivity}
      />
    ),
    matches: (
      <EngineMatchWorkspace
        visible={visible && tool === 'matches'}
        onOpenMatch={onOpenMatch}
      />
    ),
    backup: <DatabaseBackupWorkspace />,
    versions: visible && tool === 'versions' && <VersionInfo />,
  };

  return (
    <>
      <TabList
        as="nav"
        className="text-navigation extras-navigation"
        ariaLabel={benchmark('navigation')}
        active={tool}
        onSelect={setTool}
        tabs={tools.map((item) => ({
          value: item,
          label: labels[item],
          id: `extras-tab-${item}`,
          controls: `extras-panel-${item}`,
        }))}
      />
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

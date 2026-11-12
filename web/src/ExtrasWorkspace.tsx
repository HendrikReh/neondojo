import BenchmarkWorkspace from './BenchmarkWorkspace';
import { useBenchmarkText } from './benchmark-messages';
import EngineMatchWorkspace from './EngineMatchWorkspace';
import { engineMatchText } from './engine-match-messages';
import { useI18n } from './i18n';
import VersionInfo from './VersionInfo';

export type ExtrasTool = 'benchmark' | 'matches' | 'versions';

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
  const { locale, t } = useI18n();

  return (
    <>
      <nav
        className="text-navigation benchmark-navigation"
        aria-label={benchmark('navigation')}
      >
        <button
          type="button"
          aria-current={tool === 'benchmark' ? 'page' : undefined}
          onClick={() => setTool('benchmark')}
        >
          {benchmark('benchmark')}
        </button>
        <button
          type="button"
          aria-current={tool === 'matches' ? 'page' : undefined}
          onClick={() => setTool('matches')}
        >
          {engineMatchText(locale, 'title')}
        </button>
        <button
          type="button"
          aria-current={tool === 'versions' ? 'page' : undefined}
          onClick={() => setTool('versions')}
        >
          {t('versions.title')}
        </button>
      </nav>
      <div hidden={tool !== 'benchmark'}>
        <BenchmarkWorkspace
          visible={visible && tool === 'benchmark'}
          defaultPlies={defaultPlies}
          playPlies={playPlies}
        />
      </div>
      <div hidden={tool !== 'matches'}>
        <EngineMatchWorkspace visible={visible && tool === 'matches'} />
      </div>
      {visible && tool === 'versions' && <VersionInfo />}
    </>
  );
}

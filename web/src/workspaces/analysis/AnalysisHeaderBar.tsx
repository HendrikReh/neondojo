import type { Job, Match } from '../../shared/http/api';
import type { Theme } from '../../shared/board/Board';
import { TabList, type RovingTabsController } from '../../shared/ui/TabList';
import { useI18n } from '../../shared/i18n/i18n';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import { useStatisticsText } from '../../shared/review/review-statistics-messages';
import AnalysisMatchHeader from './AnalysisMatchHeader';
import AnalysisCatalogSelect from './AnalysisCatalogSelect';
import { useAnalysisMatchHeaderText } from './analysis-match-header-messages';
import GameSelector from './GameSelector';
import MatchAnalysisHeader from './match/MatchAnalysisHeader';
import PositionCalculationControls from './PositionCalculationControls';
import AnalysisRunningJobs from './AnalysisRunningJobs';
import type { AnalysisCatalog } from './useAnalysisCatalog';
import type { PositionCalculation } from './usePositionCalculation';
import type { MatchAnalysisEngine } from './match/useMatchAnalysis';
import type { AnalysisDetail } from './AnalysisWorkspace';
import type { useGameNavigation } from '../../app/useGameNavigation';

type GameItem = ReturnType<typeof useGameNavigation>['games'][number];

export default function AnalysisHeaderBar({
  match,
  games,
  gameNo,
  detail,
  availableTabs,
  detailRoving,
  sourceAvailable,
  theme,
  active,
  notesSuspended,
  metadataRevision,
  analysisRevision,
  catalogRevision,
  catalog,
  defaultEngine,
  defaultPlies,
  configured,
  analysisInstallationKey,
  positionCalculation,
  runningJobs,
  onSelectGame,
  onDetail,
  onRefreshPositionJobs,
  onTrackJob,
  onAnalyzeMatch,
}: {
  match: Match;
  games: GameItem[];
  gameNo?: number;
  detail: AnalysisDetail;
  availableTabs: readonly AnalysisDetail[];
  detailRoving: RovingTabsController<AnalysisDetail>;
  sourceAvailable: boolean;
  theme: Theme;
  active: boolean;
  notesSuspended: boolean;
  metadataRevision?: number;
  analysisRevision?: string;
  catalogRevision: string;
  catalog: AnalysisCatalog;
  defaultEngine: MatchAnalysisEngine;
  defaultPlies: number;
  configured: boolean;
  analysisInstallationKey?: string;
  positionCalculation?: PositionCalculation;
  runningJobs: Job[];
  onSelectGame: (gameNo: number) => void;
  onDetail: (detail: AnalysisDetail) => void;
  onRefreshPositionJobs: () => void;
  onTrackJob: (job: Job | undefined) => void;
  onAnalyzeMatch?: (match: Match) => void;
}) {
  const { t } = useI18n();
  const w = useWorkspaceText();
  const statistics = useStatisticsText();
  const headerText = useAnalysisMatchHeaderText();

  return (
    <>
      <div className="analysis-header-row">
        <AnalysisMatchHeader
          match={match}
          selectedAnalysis={catalog.selected}
          metadataRevision={metadataRevision}
          analysisRevision={analysisRevision}
          theme={theme}
          active={active && !notesSuspended}
        />
        <AnalysisCatalogSelect
          key={match.id}
          catalog={catalog}
          onDeleted={(entry) => {
            if (entry.scope === 'position') onRefreshPositionJobs();
          }}
        />
      </div>
      {!sourceAvailable && (
        <p className="analysis-retained-context" role="status">
          {headerText('retainedPosition')}
        </p>
      )}
      <div className="analysis-commandbar">
        <GameSelector
          games={games}
          selected={gameNo}
          onSelect={onSelectGame}
        />
        <TabList<AnalysisDetail>
          className="inspector-tabs analysis-detail-navigation text-navigation text-navigation--compact"
          ariaLabel={w('positionDetails')}
          active={detail}
          onSelect={onDetail}
          roving={detailRoving}
          tabs={availableTabs.map((tab) => ({
            value: tab,
            id: `inspector-tab-${tab}`,
            controls: `inspector-panel-${tab}`,
            label:
              tab === 'overview'
                ? t('common.checker')
                : tab === 'errors'
                  ? t('common.cube')
                  : tab === 'statistics'
                    ? statistics('tab')
                    : w(tab),
          }))}
        />
        {sourceAvailable ? (
          <MatchAnalysisHeader
            key={match.id}
            match={match}
            gameNo={gameNo}
            defaultEngine={defaultEngine}
            defaultPlies={defaultPlies}
            configured={configured}
            analysisInstallationKey={analysisInstallationKey}
            analysisRevision={catalogRevision}
            catalog={catalog}
            positionCalculation={positionCalculation}
            onJob={onTrackJob}
            onAccepted={(job) => {
              onTrackJob(job);
              catalog.refresh();
            }}
            onAnalyzeMatch={onAnalyzeMatch && (() => onAnalyzeMatch(match))}
          />
        ) : (
          <div className="position-analysis-actions">
            {positionCalculation && (
              <PositionCalculationControls calculation={positionCalculation} />
            )}
          </div>
        )}
        <AnalysisRunningJobs
          jobs={runningJobs}
          displayedJobId={catalog.selected?.job?.id}
        />
      </div>
    </>
  );
}

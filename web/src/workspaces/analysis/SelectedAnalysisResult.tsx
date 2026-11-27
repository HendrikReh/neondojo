import type { Match, Position } from '../../shared/http/api';
import type { PreviewCandidate } from '../../shared/review/candidate-preview';
import { formatPly } from '../../shared/format/ply';
import CandidateTable from './candidates/CandidateTable';
import SourceAnalysisView from './candidates/SourceAnalysisView';
import { nativeCheckerCandidates } from './candidates/candidate-comparison';
import { BoardDecisionComparison } from './BoardDecisionReview';
import type { PositionAnalysisResult } from './analysis-catalog-api';
import { useAnalysisCatalogText } from './analysis-catalog-messages';
import { useCandidateText } from './candidates/candidate-messages';

export function SelectedAnalysisResult({
  result,
  selected,
  alternatives,
  onPreview,
  selectedPreview,
  position,
  playerName,
  players,
}: {
  result?: PositionAnalysisResult | null;
  selected: boolean;
  alternatives: boolean;
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
  position?: Position;
  playerName?: string;
  players?: Pick<Match, 'white' | 'black'>;
}) {
  const text = useAnalysisCatalogText();
  const candidateText = useCandidateText();
  const analysis = result?.analysis;
  const decision = result?.decision;
  const candidates = nativeCheckerCandidates(result);
  const depth = analysis ? formatPly(analysis.plies) : result?.depth;
  const source = [analysis?.engine ?? result?.engine, depth]
    .filter(Boolean)
    .join(' · ');
  if (candidates.length)
    return (
      <CandidateTable
        candidates={candidates.map((candidate) => ({
          ...candidate,
          depth: candidate.depth ?? depth,
        }))}
        source={source}
        depth={depth}
        played={decision?.played ?? position?.notation}
        position={position}
        playerName={playerName}
        onPreview={onPreview}
        selectedPreview={selectedPreview}
      />
    );
  if (result?.records.length)
    return (
      <SourceAnalysisView
        records={result.records}
        source="file"
        onPreview={onPreview}
        selectedPreview={selectedPreview}
        position={position}
        playerName={playerName}
        players={players}
      />
    );
  if (decision)
    return (
      <BoardDecisionComparison
        played={decision.played}
        best={decision.best}
        equities={{
          played: decision.played_equity,
          best: decision.best_equity,
        }}
        loss={
          decision.evaluation_status === 'evaluated'
            ? decision.equity_loss
            : null
        }
        context={candidateText('normalizedCubeful')}
        source={source}
      />
    );
  return (
    <div className="panel-empty">
      <p>{text(selected ? 'missing' : 'none')}</p>
      {alternatives && <p>{text('alternatives')}</p>}
    </div>
  );
}

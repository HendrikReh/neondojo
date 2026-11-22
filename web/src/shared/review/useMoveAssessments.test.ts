import { describe, expect, it } from 'vitest';
import type { Job } from '../http/api';
import {
  assessmentIdentity,
  assessmentRequestPath,
  assessmentItems,
  type GameMoveAssessments,
} from './useMoveAssessments';

const job = {
  id: 'job',
  kind: 'match',
  match_uuid: 'match',
  game_no: null,
  completed_games: 1,
  status: 'running',
} as Job;
const scope = { matchId: 'match', gameNo: 2, job };
const item = {
  position_id: 'position',
  severity: 'bad',
  equity_loss: 0.08,
  source: 'native',
  engine: 'GNU Backgammon',
  depth: '2-ply',
  depth_plies: 2,
  job_id: 'job',
} as const;
const data: GameMoveAssessments = {
  match_id: 'match',
  game_no: 2,
  job_id: 'job',
  items: [item],
};

describe('game move-assessment requests', () => {
  it('uses one game request and the explicitly selected native job', () => {
    expect(assessmentRequestPath(scope)).toBe(
      '/matches/match/move-assessments?game_no=2&job_id=job',
    );
    expect(assessmentRequestPath({ matchId: 'match', gameNo: 2 })).toBe(
      '/matches/match/move-assessments?game_no=2',
    );
  });
  it('waits for a matching game and job scope', () => {
    expect(assessmentRequestPath({})).toBeUndefined();
    expect(assessmentRequestPath({ matchId: 'match' })).toBeUndefined();
    expect(
      assessmentRequestPath({ matchId: 'match', gameNo: 0 }),
    ).toBeUndefined();
    expect(
      assessmentRequestPath({ ...scope, job: { ...job, match_uuid: 'other' } }),
    ).toBeUndefined();
    expect(
      assessmentRequestPath({ ...scope, job: { ...job, game_no: 1 } }),
    ).toBeUndefined();
    expect(
      assessmentRequestPath({ ...scope, job: { ...job, kind: 'analysis' } }),
    ).toBeUndefined();
  });
  it('changes identity for game, selected job, committed checkpoint and terminal status', () => {
    const identity = assessmentIdentity(scope);
    for (const next of [
      { ...scope, gameNo: 1 },
      { ...scope, matchId: 'other' },
      { ...scope, job: { ...job, id: 'other' } },
      { ...scope, job: { ...job, completed_games: 2 } },
      { ...scope, job: { ...job, status: 'completed' as const } },
    ])
      expect(assessmentIdentity(next)).not.toBe(identity);
  });
  it('refreshes identity after an individual hint completes without changing request scope', () => {
    const completedHint = {
      ...scope,
      analysisRevision: 'hint:completed:2026-11-09T00:35:00Z',
    };
    const updatedHint = {
      ...completedHint,
      analysisRevision: 'hint:completed:2026-11-09T00:36:00Z',
    };
    expect(assessmentIdentity(completedHint)).not.toBe(
      assessmentIdentity(scope),
    );
    expect(assessmentIdentity(updatedHint)).not.toBe(
      assessmentIdentity(completedHint),
    );
    expect(assessmentRequestPath(completedHint)).toBe(
      assessmentRequestPath(scope),
    );
    expect(assessmentRequestPath(updatedHint)).toBe(
      assessmentRequestPath(scope),
    );
  });
  it('rejects stale game, match and selected-job responses', () => {
    expect(assessmentItems(scope, data)).toEqual({ position: item });
    for (const stale of [
      { ...data, match_id: 'other' },
      { ...data, game_no: 1 },
      { ...data, job_id: 'other' },
      { ...data, job_id: null },
    ])
      expect(assessmentItems(scope, stale)).toEqual({});
  });
  it('keeps unknown, invalid and non-finite assessments neutral', () => {
    const malformed = [
      { ...item, severity: 'unknown' },
      { ...item, equity_loss: NaN },
      { ...item, equity_loss: Infinity },
      { ...item, equity_loss: -0.1 },
      { ...item, source: 'unknown' },
      { ...item, engine: '' },
      { ...item, position_id: '' },
    ];
    for (const invalid of malformed)
      expect(
        assessmentItems(scope, {
          ...data,
          items: [invalid],
        } as GameMoveAssessments),
      ).toEqual({});
    expect(
      assessmentItems(scope, {
        ...data,
        items: [{ ...item, severity: 'none', equity_loss: 0 }],
      }),
    ).toEqual({ position: { ...item, severity: 'none', equity_loss: 0 } });
  });
});

it('uses the shared catalog identity and rejects late responses from another analysis', () => {
  const selected = {
    matchId: 'match',
    gameNo: 2,
    analysisId: 'imported:match',
  };
  expect(assessmentRequestPath(selected)).toBe(
    '/matches/match/move-assessments?game_no=2&analysis_id=imported%3Amatch',
  );
  expect(assessmentIdentity(selected)).not.toBe(
    assessmentIdentity({ ...selected, analysisId: 'job:other' }),
  );
  expect(
    assessmentItems(selected, {
      ...data,
      job_id: null,
      analysis_id: 'job:other',
    }),
  ).toEqual({});
  expect(
    assessmentItems(selected, {
      ...data,
      job_id: null,
      analysis_id: 'imported:match',
    }),
  ).toEqual({ position: item });
});

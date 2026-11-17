import { el } from '../utils.js';
import type { UnitsStatus, Verdict } from '../types.js';

export const listReady = (status: UnitsStatus | null, listProblem: boolean): boolean =>
  listLoaded(status) && !listProblem;

export const listLoaded = (status: UnitsStatus | null): status is UnitsStatus =>
  Boolean(status?.build);

export interface VerifyState {
  status: UnitsStatus | null;
  mode: 'online' | 'file';
  net: 'unknown' | 'online' | 'offline';
  cameras: number | null;
  problem: string | null;
  listProblem: boolean;
  stream: MediaStream | null;
  decodeTimer: number | null;
  silentWait: number | null;
  silentSaid: boolean;
  stopWatch: (() => void) | null;
  verdict: Verdict | null;
  challengeText: string | null;
  drawing: Promise<void> | null;
  answered: boolean;
  fetching: boolean;
  frames: number;
  slowSaid: boolean;
  rate: { at: number; frames: number };
  openedAt: number;
  generation: number;
  broken: Map<string, string>;
}

export const verify: VerifyState = {
  status: null,
  mode: 'online',
  net: 'unknown',
  cameras: null,
  problem: null,
  listProblem: false,
  stream: null,
  decodeTimer: null,
  silentWait: null,
  silentSaid: false,
  stopWatch: null,
  verdict: null,
  challengeText: null,
  drawing: null,
  answered: false,
  fetching: false,
  frames: 0,
  slowSaid: false,
  rate: { at: 0, frames: 0 },
  openedAt: 0,
  generation: 0,
  broken: new Map(),
};

export const renderScan = (): void => {
  const problem = el('scan-problem');
  problem.hidden = !verify.problem;
  problem.textContent = verify.problem ?? '';
};

export const say = (problem: string | null): void => {
  verify.problem = problem;
  renderScan();
};

import { el } from '../utils.js';
import type { UnitsStatus, Verdict } from '../types.js';

export const listReady = (status: UnitsStatus | null, listProblem: boolean): boolean =>
  listLoaded(status) && !listProblem;

export const listLoaded = (status: UnitsStatus | null): status is UnitsStatus =>
  Boolean(status?.build);

export interface VerifyState {
  status: UnitsStatus | null;
  mode: 'online' | 'file';
  problem: string | null;
  listProblem: boolean;
  verdict: Verdict | null;
  challengeText: string | null;
  drawing: Promise<void> | null;
  answered: boolean;
  fetching: boolean;
}

export const verify: VerifyState = {
  status: null,
  mode: 'online',
  problem: null,
  listProblem: false,
  verdict: null,
  challengeText: null,
  drawing: null,
  answered: false,
  fetching: false,
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

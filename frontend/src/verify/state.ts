/* The verify session, and the note that says which part of it is missing. */

import { el } from '../utils.js';
import type { UnitsStatus, Verdict } from '../types.js';

export interface VerifyState {
  /** The list the screen describes, straight from `units_status`. */
  status: UnitsStatus | null;
  mode: 'online' | 'file';
  net: 'unknown' | 'online' | 'offline';
  /** Why the camera or the list is not available yet: said in the note box, never in a dialog. */
  problem: string | null;
  problemDetail: string | null;
  stream: MediaStream | null;
  decodeTimer: number | null;
  silentWait: number | null;
  silentSaid: boolean;
  stopWatch: (() => void) | null;
  verdict: Verdict | null;
  drawn: boolean;
  fetching: boolean;
  frames: number;
  slowSaid: boolean;
  rate: { at: number; frames: number };
  openedAt: number;
  generation: number;
}

export const verify: VerifyState = {
  status: null,
  mode: 'online',
  net: 'unknown',
  problem: null,
  problemDetail: null,
  stream: null,
  decodeTimer: null,
  silentWait: null,
  silentSaid: false,
  stopWatch: null,
  verdict: null,
  drawn: false,
  fetching: false,
  frames: 0,
  slowSaid: false,
  rate: { at: 0, frames: 0 },
  openedAt: 0,
  generation: 0,
};

/** The scan screen speaks for itself: whatever goes wrong with the camera happens in front of it. */
export const renderScan = (): void => {
  const problem = el('scan-problem');
  problem.hidden = !verify.problem;
  problem.textContent = verify.problem ?? '';
  const detail = el('scan-problem-detail');
  detail.hidden = !verify.problemDetail;
  detail.textContent = verify.problemDetail ?? '';
};

export const say = (problem: string | null, detail: string | null = null): void => {
  verify.problem = problem;
  verify.problemDetail = detail;
  renderScan();
};

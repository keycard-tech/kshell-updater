/* The verify session, and the note that says which part of it is missing. */

import { el } from '../utils.js';
import type { UnitsStatus, Verdict } from '../types.js';

/**
 * `units_status` answers with an empty status rather than an error when nothing has been loaded, so
 * the build number is what says a list is really here. Everything the screen allows or blurs hangs
 * on it.
 */
export const listLoaded = (status: UnitsStatus | null): status is UnitsStatus =>
  Boolean(status?.build);

export interface VerifyState {
  /** The list the screen describes, straight from `units_status`. */
  status: UnitsStatus | null;
  mode: 'online' | 'file';
  net: 'unknown' | 'online' | 'offline';
  /** Why the camera or the list is not available yet: said on the screen, never in a dialog. */
  problem: string | null;
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
};

export const say = (problem: string | null): void => {
  verify.problem = problem;
  renderScan();
};

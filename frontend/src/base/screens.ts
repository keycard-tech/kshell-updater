/* One screen visible at a time. */

import { el } from '../utils.js';
import { state } from './state.js';

export const SCREENS = [
  'screen-home',
  'screen-ready',
  'screen-transfer',
  'screen-done',
  'screen-failed',
  'screen-verify',
  'screen-scan',
  'screen-verdict',
] as const;

export type Screen = (typeof SCREENS)[number];

export const show = (screen: Screen): void => {
  state.screen = screen;
  for (const id of SCREENS) el(id).hidden = id !== screen;
  el('statusbar').hidden = screen === 'screen-ready' || screen === 'screen-transfer';
};

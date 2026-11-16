import { openExternal, unitsCheck } from '../base/bridge.js';
import { show } from '../base/screens.js';
import { images } from '../config.js';
import * as text from '../strings.js';
import type { Verdict } from '../types.js';
import { el, reason } from '../utils.js';
import { stopCamera } from './camera.js';
import { openVerify } from './screen.js';
import { verify } from './state.js';

export const answer = async (code: string): Promise<void> => {
  stopCamera();
  let verdict: Verdict;
  try {
    verdict = await unitsCheck(code);
  } catch (error) {
    verdict = { outcome: 'refused', reason: reason(error) };
  }
  verify.verdict = verdict;
  verify.answered = true;
  renderVerdict();
  show('screen-verdict');
};

const shown = (): { icon: string } & text.Shown => {
  const verdict: Verdict = verify.verdict ?? {
    outcome: 'refused',
    reason: text.verdict.nothingScanned,
  };
  if (verdict.outcome === 'authentic') {
    return { icon: images.success, ...text.verdict.authentic() };
  }
  return verdict.outcome === 'notRegistered'
    ? { icon: images.untrustworthy, ...text.verdict.notRegistered(verdict.uid) }
    : { icon: images.failed, ...text.verdict.refused(verdict.reason) };
};

export const renderVerdict = (): void => {
  const result = shown();
  el<HTMLImageElement>('verdict-icon').src = result.icon;
  el('verdict-title').textContent = result.title;
  el('verdict-text').textContent = result.text;
  const sub = el('verdict-sub');
  sub.hidden = !result.sub;
  sub.textContent = result.sub;
  el<HTMLButtonElement>('btn-verdict').textContent = result.button;
};

export const verdictButton = (): void => {
  const outcome = verify.verdict?.outcome;
  if (outcome !== 'notRegistered') {
    if (outcome !== 'authentic') void openVerify();
    else show('screen-home');
    return;
  }
  openExternal(text.links.verifyHelp).catch((error: unknown) =>
    console.warn(`could not open ${text.links.verifyHelp}: ${reason(error)}`),
  );
};

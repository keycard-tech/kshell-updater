import { openExternal, unitsCheck } from '../base/bridge.js';
import { show } from '../base/screens.js';
import { images } from '../config.js';
import { human } from '../errors.js';
import * as text from '../strings.js';
import type { Verdict } from '../types.js';
import { el, reason } from '../utils.js';
import { stopScan } from './scan.js';
import { openVerify } from './screen.js';
import { verify } from './state.js';

export const answer = async (code: string): Promise<void> => {
  stopScan();
  let verdict: Verdict;
  try {
    verdict = await unitsCheck(code);
  } catch (error) {
    console.warn('the scanned answer was refused:', reason(error));
    verdict = { outcome: 'refused', reason: human(reason(error), 'answer') };
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
    : {
        icon: images.failed,
        ...text.verdict.refused(verify.verdict ? human(verdict.reason, 'answer') : verdict.reason),
      };
};

const renderVerdict = (): void => {
  const result = shown();
  el<HTMLImageElement>('verdict-icon').src = result.icon;
  el('verdict-title').textContent = result.title;
  el('verdict-text').textContent = result.text;
  const sub = el('verdict-sub');
  sub.hidden = !result.sub;
  sub.textContent = result.sub ?? '';
  el<HTMLButtonElement>('btn-verdict').textContent = result.button;
  el('btn-verdict-guide').hidden = verify.verdict?.outcome !== 'notRegistered';
};

export const verdictButton = (): void => {
  if (verify.verdict?.outcome === 'refused') void openVerify();
  else show('screen-home');
};

export const verdictGuide = (): void => {
  openExternal(text.links.verifyHelp).catch((error: unknown) =>
    console.warn(`could not open ${text.links.verifyHelp}: ${reason(error)}`),
  );
};

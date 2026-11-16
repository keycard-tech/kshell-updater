/* The challenge screen: the code, the list it is checked against, the note in between. */

import {
  chooseFile,
  unitsChallenge,
  unitsFetch,
  unitsStatus,
  unitsUseFile,
} from '../base/bridge.js';
import { show } from '../base/screens.js';
import * as text from '../strings.js';
import { asHtml, el, reason } from '../utils.js';
import { listLine } from '../views/verify.js';
import { verify } from './state.js';

export const openVerify = async (): Promise<void> => {
  show('screen-verify');
  verify.problem = null;
  verify.drawn = false;
  verify.status = (await unitsStatus().catch(() => null)) ?? null;
  renderVerify();
  // The challenge is drawn before anything asks the network for a list: it does not depend on which
  // list the answer is checked against, and an empty tile looks like the failure it is not.
  await ensureChallenge();
  await fetchList();
  renderVerify();
};

/** One challenge per visit: drawing a second would invalidate what the Shell is already signing. */
const ensureChallenge = async (): Promise<void> => {
  if (verify.drawn) return;
  try {
    const { svg } = await unitsChallenge();
    // The SVG is written here rather than fetched, so painting it needs no remote origin.
    el('verify-qr').innerHTML = svg;
    verify.drawn = true;
  } catch (error) {
    verify.problem = reason(error);
  }
};

/** A cached list stays useful when the fetch fails. */
export const fetchList = async (): Promise<void> => {
  if (verify.mode !== 'online') return;
  verify.fetching = true;
  renderVerify();
  try {
    verify.status = await unitsFetch();
    verify.net = 'online';
    verify.problem = null;
  } catch (error) {
    verify.net = 'offline';
    verify.status = (await unitsStatus().catch(() => null)) ?? verify.status;
    if (!verify.status) verify.problem = reason(error);
  }
  verify.fetching = false;
};

export const useOwnList = async (): Promise<void> => {
  try {
    // Picking the file is the whole request: it is read and verified here, not behind a button.
    const path = await chooseFile({
      title: text.dialogs.units,
      filterName: text.dialogs.unitsFilter,
      extensions: [text.dialogs.extension],
    });
    if (path) {
      verify.status = await unitsUseFile(path);
      verify.mode = 'file';
      verify.problem = null;
      await ensureChallenge();
    }
  } catch (error) {
    verify.problem = reason(error);
  }
  renderVerify();
};

export const renderVerify = (): void => {
  const loaded = Boolean(verify.status);

  const online = el<HTMLButtonElement>('verify-list-online');
  online.classList.toggle('ksh__seg--on', loaded && verify.mode === 'online');
  el('verify-list-file').classList.toggle('ksh__seg--on', loaded && verify.mode === 'file');
  // With nothing on the computer, "Fetched list" is not a choice to make: the button below fetches it.
  online.disabled = !loaded;
  el('verify-net').textContent =
    loaded && verify.net !== 'offline' ? text.verify.online : text.verify.noNetwork;

  el('verify-list-line').innerHTML = asHtml(listLine(verify.status, verify.fetching));

  // Only the reason something failed: the line above already says what to do about it.
  el('verify-note').hidden = !verify.problem;
  el('verify-note-text').textContent = verify.problem ?? '';

  const button = el<HTMLButtonElement>('btn-camera');
  button.textContent = loaded
    ? text.verify.enableCamera
    : verify.fetching
      ? text.verify.fetchingList
      : text.verify.loadList;
  button.disabled = verify.fetching;
  // Solid orange means "go", and this screen can only say that with a list checked against.
  button.classList.toggle('ksh__btn--primary', loaded);
};

import {
  chooseFile,
  unitsChallenge,
  unitsFetch,
  unitsStatus,
  unitsUseFile,
} from '../base/bridge.js';
import { state } from '../base/state.js';
import { show } from '../base/screens.js';
import { human } from '../errors.js';
import * as text from '../strings.js';
import { el, reason } from '../utils.js';
import { listLine } from '../views/verify.js';
import { listLoaded, listReady, verify } from './state.js';

export const openVerify = async (): Promise<void> => {
  show('screen-verify');
  verify.problem = null;
  verify.listProblem = false;
  verify.status = (await unitsStatus().catch(() => null)) ?? null;
  renderVerify();
  if (verify.challengeText === null || verify.answered) {
    await ensureChallenge(verify.answered);
  }
  await fetchList();
  renderVerify();
};

export const ensureChallenge = async (renew: boolean): Promise<void> => {
  if (verify.drawing) return verify.drawing;
  verify.drawing = (async () => {
    try {
      const challenge = await unitsChallenge(renew);
      verify.challengeText = challenge.text;
      el('verify-qr').innerHTML = challenge.svg;
      if (renew) verify.answered = false;
    } catch (error) {
      console.warn('the challenge would not be drawn:', reason(error));
      verify.problem = human(reason(error));
    } finally {
      verify.drawing = null;
    }
  })();
  return verify.drawing;
};

export const fetchList = async (): Promise<void> => {
  if (verify.mode !== 'online' || state.offline) return;
  verify.fetching = true;
  renderVerify();
  try {
    verify.status = await unitsFetch();
    verify.problem = null;
    verify.listProblem = false;
  } catch (error) {
    verify.status = (await unitsStatus().catch(() => null)) ?? verify.status;
    console.warn('the units list would not be fetched:', reason(error));
    verify.problem = human(reason(error));
    verify.listProblem = true;
  }
  verify.fetching = false;
};

export const ensureList = async (): Promise<void> => {
  if (listLoaded(verify.status)) return;
  await fetchList();
  renderVerify();
};

export const useOwnList = async (): Promise<void> => {
  try {
    const path = await chooseFile({
      title: text.dialogs.units,
      filterName: text.dialogs.unitsFilter,
      extensions: [text.dialogs.extension],
    });
    if (path) {
      verify.status = await unitsUseFile(path);
      verify.mode = 'file';
      verify.problem = null;
      verify.listProblem = false;
    }
  } catch (error) {
    console.warn('the own list would not be read:', reason(error));
    verify.problem = human(reason(error));
    verify.listProblem = true;
  }
  renderVerify();
};

export const renderVerify = (): void => {
  const loaded = listReady(verify.status, verify.listProblem);

  const online = el<HTMLButtonElement>('verify-list-online');
  online.classList.toggle('ksh__seg--on', loaded && verify.mode === 'online');
  el('verify-list-file').classList.toggle('ksh__seg--on', loaded && verify.mode === 'file');
  online.disabled = state.offline;
  el('verify-list-line').textContent = listLine(verify.status, verify.fetching);
  el('verify-qr-tile').classList.toggle('ksh__qr-tile--blurred', !loaded);
  el('verify-qr-note').hidden = loaded;

  const note = verify.problem;
  el('verify-note').hidden = !note;
  el('verify-note-text').textContent = note ?? '';
  el('btn-verify-update').textContent =
    state.device && state.config ? text.verify.updateShell : text.verify.goHome;

  const button = el<HTMLButtonElement>('btn-camera');
  button.textContent = verify.fetching ? text.verify.fetchingList : text.verify.enableCamera;
  button.disabled = !loaded || verify.fetching;
  el('btn-units-code').hidden = loaded || verify.fetching;
};

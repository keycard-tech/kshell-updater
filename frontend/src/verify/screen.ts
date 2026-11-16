import {
  chooseFile,
  unitsChallenge,
  unitsFetch,
  unitsStatus,
  unitsUseFile,
} from '../base/bridge.js';
import { show } from '../base/screens.js';
import * as text from '../strings.js';
import { el, reason } from '../utils.js';
import { listLine } from '../views/verify.js';
import { listReady, verify } from './state.js';

export const openVerify = async (): Promise<void> => {
  show('screen-verify');
  verify.problem = null;
  verify.drawn = false;
  verify.listProblem = false;
  verify.status = (await unitsStatus().catch(() => null)) ?? null;
  renderVerify();
  await ensureChallenge();
  await fetchList();
  renderVerify();
};

const ensureChallenge = async (): Promise<void> => {
  if (verify.drawn) return;
  try {
    const { svg } = await unitsChallenge();
    el('verify-qr').innerHTML = svg;
    verify.drawn = true;
  } catch (error) {
    verify.problem = reason(error);
  }
};

export const fetchList = async (): Promise<void> => {
  if (verify.mode !== 'online') return;
  verify.fetching = true;
  renderVerify();
  try {
    verify.status = await unitsFetch();
    verify.net = 'online';
    verify.problem = null;
    verify.listProblem = false;
  } catch (error) {
    verify.net = 'offline';
    verify.status = (await unitsStatus().catch(() => null)) ?? verify.status;
    verify.problem = reason(error);
    verify.listProblem = true;
  }
  verify.fetching = false;
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
      await ensureChallenge();
    }
  } catch (error) {
    verify.problem = reason(error);
    verify.listProblem = true;
  }
  renderVerify();
};

export const renderVerify = (): void => {
  const loaded = listReady(verify.status, verify.listProblem);

  const online = el<HTMLButtonElement>('verify-list-online');
  online.classList.toggle('ksh__seg--on', loaded && verify.mode === 'online');
  el('verify-list-file').classList.toggle('ksh__seg--on', loaded && verify.mode === 'file');
  online.disabled = !loaded;
  el('verify-list-line').textContent = listLine(verify.status, verify.fetching);
  el('verify-qr-tile').classList.toggle('ksh__qr-tile--blurred', !loaded);
  el('verify-qr-note').hidden = loaded;
  el('verify-note').hidden = !verify.problem;
  el('verify-note-text').textContent = verify.problem ?? '';

  const button = el<HTMLButtonElement>('btn-camera');
  button.textContent = loaded
    ? text.verify.enableCamera
    : verify.fetching
      ? text.verify.fetchingList
      : text.verify.loadList;
  button.disabled = verify.fetching;
  button.classList.toggle('ksh__btn--primary', loaded);
};

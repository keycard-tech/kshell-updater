import {
  chooseFile,
  unitsChallenge,
  unitsFetch,
  unitsStatus,
  unitsUseFile,
} from '../base/bridge.js';
import { state } from '../base/state.js';
import { discoverCameras } from './camera.js';
import { show } from '../base/screens.js';
import { human } from '../errors.js';
import * as text from '../strings.js';
import { el, reason } from '../utils.js';
import { listLine } from '../views/verify.js';
import { listReady, verify } from './state.js';

export const openVerify = async (): Promise<void> => {
  show('screen-verify');
  verify.problem = null;
  void discoverCameras();
  verify.listProblem = false;
  verify.status = (await unitsStatus().catch(() => null)) ?? null;
  renderVerify();
  if (verify.challengeText === null || verify.answered) {
    await ensureChallenge(verify.answered);
  }
  await fetchList();
  renderVerify();
};

/** The code the Shell signs belongs to the verify session: a list arriving, a camera restarting or a
    render never replaces it, because the Shell may already be signing that one. Only a session that
    ended asks for a fresh code, and a second call while one is on its way joins it. */
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
    console.warn('the units list would not be fetched:', reason(error));
    verify.problem = human(reason(error));
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
  online.disabled = !loaded;
  el('verify-list-line').textContent = listLine(verify.status, verify.fetching);
  el('verify-qr-tile').classList.toggle('ksh__qr-tile--blurred', !loaded);
  el('verify-qr-note').hidden = loaded;

  const note = verify.cameras === 0 ? text.camera.none : verify.problem;
  el('verify-note').hidden = !note;
  el('verify-note-text').textContent = note ?? '';
  el('btn-verify-update').textContent =
    state.device && state.config ? text.verify.updateShell : text.verify.goHome;

  const button = el<HTMLButtonElement>('btn-camera');
  // The press waits for the count, because a machine with nothing to start has no good answer to it.
  // A count of zero does not disable it: an engine that cannot see the camera in front of it is a
  // failure this app has been shown making, and a press is the only thing that says which failure -
  // `no camera`, `busy`, `blocked` - rather than a button that refuses to be asked.
  let label = text.verify.enableCamera;
  if (!loaded) label = verify.fetching ? text.verify.fetchingList : text.verify.loadList;
  else if (verify.cameras === null) label = text.verify.findingCameras;
  else if (verify.cameras === 0) label = text.verify.noCamera;
  button.textContent = label;
  button.disabled = verify.fetching || (loaded && verify.cameras === null);
  button.classList.toggle('ksh__btn--primary', loaded);
};

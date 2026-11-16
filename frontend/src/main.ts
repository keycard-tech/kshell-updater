import { events, listShells, listen } from './base/bridge.js';
import { show } from './base/screens.js';
import { state } from './base/state.js';
import type { FlashProgress, Preloaded } from './types.js';
import { presencePollMs } from './config.js';
import { el } from './utils.js';
import { refresh, showWaiting } from './update/device.js';
import { keyFor } from './update/files.js';
import { loadReleases, preload, retryListing } from './update/releases.js';
import { readRecord } from './update/record.js';
import { buildRows, render } from './update/rows.js';
import {
  backToUpdates,
  cancelTransfer,
  renderTransfer,
  retryTransfer,
  startTransfer,
} from './update/transfer.js';
import { cameraStart, stopCamera } from './verify/camera.js';
import {
  ensureChallenge,
  fetchList,
  openVerify,
  renderVerify,
  useOwnList,
} from './verify/screen.js';
import { renderScan, verify } from './verify/state.js';
import { answer, verdictButton, verdictGuide } from './verify/verdict.js';

buildRows();

el<HTMLButtonElement>('btn-transfer').addEventListener('click', () => void startTransfer());
el<HTMLButtonElement>('btn-cancel').addEventListener('click', cancelTransfer);
el<HTMLButtonElement>('btn-retry').addEventListener('click', () => void retryTransfer());
el<HTMLButtonElement>('btn-failed-back').addEventListener('click', () => {
  backToUpdates();
  void refresh();
});
el<HTMLButtonElement>('btn-ready-verify').addEventListener('click', () => void openVerify());
el<HTMLButtonElement>('card-update').addEventListener('click', openUpdate);
el<HTMLButtonElement>('btn-done').addEventListener('click', () => {
  show('screen-home');
  void refresh();
});

el<HTMLButtonElement>('card-verify').addEventListener('click', () => void openVerify());
el<HTMLButtonElement>('btn-verdict-guide').addEventListener('click', verdictGuide);
el<HTMLButtonElement>('btn-verify-update').addEventListener('click', () => {
  if (state.device && state.config) {
    openUpdate();
    return;
  }
  show('screen-home');
  void refresh();
});
el<HTMLButtonElement>('verify-list-online').addEventListener('click', () => void useOnlineList());
el<HTMLButtonElement>('verify-list-file').addEventListener('click', () => void useOwnList());
el<HTMLButtonElement>('btn-camera').addEventListener('click', () => void startCamera());
el<HTMLButtonElement>('btn-scan-back').addEventListener('click', () => {
  stopCamera();
  show('screen-verify');
});
el<HTMLSelectElement>('scan-camera').addEventListener('change', (event) =>
  void switchCamera((event.target as HTMLSelectElement).value),
);
el<HTMLButtonElement>('btn-verdict').addEventListener('click', verdictButton);


// Whether a Shell is attached decides what the verify screen calls its way out, so both presence
// events re-render it - a person watching a camera screen should not have to leave and return.
listen(events.connected, () => {
  if (!state.busy) void refresh().then(renderVerify);
});

listen(events.disconnected, () => {
  if (state.busy) return;
  showWaiting();
  renderVerify();
});

listen<FlashProgress>(events.flashProgress, (progress) => {
  state.transfer = { done: progress.done, total: progress.total };
  renderTransfer();
});

listen<Preloaded>(events.preloaded, (file) => {
  state.cache[file.key] = { ok: file.ok, note: file.note ?? undefined };
  for (const kind of ['firmware', 'database'] as const) {
    if (keyFor(kind) === file.key) render(kind);
  }
});

async function useOnlineList(): Promise<void> {
  if (verify.mode === 'online' && verify.status) return;
  verify.mode = 'online';
  verify.problem = null;
  await fetchList();
  renderVerify();
}

async function startCamera(): Promise<void> {
  if (!verify.status) {
    await fetchList();
    renderVerify();
    return;
  }
  await ensureChallenge(false);
  await cameraStart(answer);
}

async function switchCamera(deviceId: string): Promise<void> {
  stopCamera();
  await cameraStart(answer, deviceId);
}

async function poll(): Promise<void> {
  if (state.scanning || state.busy) return;
  state.scanning = true;
  try {
    void retryListing(false);
    const found = await listShells();
    const path = found.length ? found[0].path : null;
    const seen = state.device?.path ?? null;
    if (path !== seen || (path && !state.config)) await refresh().then(renderVerify);
  } catch {
  } finally {
    state.scanning = false;
  }
}

function openUpdate(): void {
  if (!state.device || !state.config) {
    showWaiting();
    return;
  }
  show('screen-ready');
  void refresh();
  void retryListing(true);
}

showWaiting();
void readRecord()
  .then(refresh)
  .then(loadReleases)
  .then(preload);

setInterval(() => void poll(), presencePollMs);
addEventListener('online', () => void retryListing(true));

window.kshell = { scanAnswer: answer, renderScan, verify };

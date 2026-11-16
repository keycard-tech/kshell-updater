/* Wiring: which control does what, and what the Shell tells us as it happens. */

import { events, listShells, listen } from './base/bridge.js';
import { show } from './base/screens.js';
import { state } from './base/state.js';
import type { FlashProgress, Preloaded } from './types.js';
import { presencePollMs } from './config.js';
import { el } from './utils.js';
import { refresh, showWaiting } from './update/device.js';
import { keyFor } from './update/files.js';
import { loadReleases, preload } from './update/releases.js';
import { readRecord } from './update/record.js';
import { buildRows, render } from './update/rows.js';
import { cancelTransfer, renderTransfer, retryTransfer, startTransfer } from './update/transfer.js';
import { cameraStart, stopCamera } from './verify/camera.js';
import { fetchList, openVerify, renderVerify, useOwnList } from './verify/screen.js';
import { renderScan, verify } from './verify/state.js';
import { answer, verdictButton } from './verify/verdict.js';

buildRows();

el<HTMLButtonElement>('btn-transfer').addEventListener('click', () => void startTransfer());
el<HTMLButtonElement>('btn-cancel').addEventListener('click', cancelTransfer);
el<HTMLButtonElement>('btn-retry').addEventListener('click', () => void retryTransfer());
el<HTMLButtonElement>('btn-ready-verify').addEventListener('click', () => void openVerify());
el<HTMLButtonElement>('card-update').addEventListener('click', () => {
  if (!state.device || !state.config) return;
  show('screen-ready');
  void refresh();
});
el<HTMLButtonElement>('btn-done').addEventListener('click', () => {
  show('screen-home');
  void refresh();
});

el<HTMLButtonElement>('card-verify').addEventListener('click', () => void openVerify());
el<HTMLButtonElement>('btn-verify-back').addEventListener('click', () => show('screen-home'));
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

// No configuration read while a write owns the device: the Shell answers with what it runs when the
// command in flight returns.
listen(events.connected, () => {
  if (!state.busy) void refresh();
});
listen(events.disconnected, () => {
  // A firmware write ends with the Shell restarting, and the watcher sees that as an unplug. The
  // screen stays where it is: the command in flight answers with the outcome, restart included.
  if (state.busy) return;
  showWaiting();
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

/** The list decides whether the button opens the camera or fetches what the camera needs. */
async function startCamera(): Promise<void> {
  if (verify.status) {
    await cameraStart(answer);
    return;
  }
  await fetchList();
  renderVerify();
}

async function switchCamera(deviceId: string): Promise<void> {
  stopCamera();
  await cameraStart(answer, deviceId);
}

/** Polled as well as pushed: the watcher's first event can arrive before this document listens. */
async function poll(): Promise<void> {
  if (state.scanning || state.busy) return;
  state.scanning = true;
  try {
    const found = await listShells();
    const path = found.length ? found[0].path : null;
    const seen = state.device?.path ?? null;
    if (path !== seen || (path && !state.config)) await refresh();
  } catch {
    // Unreadable for a moment while USB settles; the next tick sees it.
  } finally {
    state.scanning = false;
  }
}

showWaiting();
// Read before the device, so the first render can mention a previous run.
void readRecord()
  .then(refresh)
  .then(loadReleases)
  .then(preload);

setInterval(() => void poll(), presencePollMs);

/* shots/ drives the app from outside the module graph: a scanned code, and what the camera screen says. */
window.kshell = { scanAnswer: answer, renderScan, verify };

/* The queue: what gets written, in what order, and what came back afterwards. */

import { cancelFlash, flashImage } from '../base/bridge.js';
import { show } from '../base/screens.js';
import { state } from '../base/state.js';
import * as text from '../strings.js';
import type { ImageKind } from '../types.js';
import { asHtml, el, reason } from '../utils.js';
import { itemRow, spinner, stateWord } from '../views/transfer.js';
import { reportConnected } from './device.js';
import { isReadyToFlash, keyFor, selectedFile, versionOf } from './files.js';
import {
  dropRecord,
  intendedVersions,
  onDeviceVersions,
  writeRecord,
} from './record.js';
import { rowState, wantedKinds } from './rows.js';

const kinds = text.kinds.map((entry) => entry.kind);

/** 'Firmware 1.5.3-omni' / 'Database 20261111', as the transfer screen names them. */
export const itemLabel = (kind: ImageKind): string => {
  const name = text.title(kind);
  if (state.mode[kind] === 'file') {
    return `${name} ${state.ready[kind]?.info?.version ?? ''}`.trim();
  }
  const file = selectedFile(kind);
  const variant =
    file?.variant === null || file?.variant === undefined ? '' : `-${text.variants[file.variant] ?? ''}`;
  return `${name} ${versionOf(file)}${variant}`;
};

export const startTransfer = async (): Promise<void> => {
  const wanted = wantedKinds();
  if (state.busy || !wanted.length || wanted.some((kind) => !isReadyToFlash(kind))) return;

  // Firmware first: a token list the firmware cannot read is worse than the other way round.
  state.queue = wanted.map((kind) => {
    const status = rowState(kind).status;
    const key = keyFor(kind) ?? '';
    return {
      kind,
      key,
      label: itemLabel(kind),
      bytes: state.prepared[key]?.bytes ?? 0,
      done: false,
      // No dialog: the row's own status word is the warning that pressing the button agrees to.
      allowVariantSwitch: status === text.status.variantSwitch,
      // Nothing we offer is older: online only the newest is listed, and a file that is older stops
      // at the row. The Shell still refuses whatever it will not take.
      allowDowngrade: false,
    };
  });
  state.step = 0;
  state.transfer = null;
  state.transferError = null;
  show('screen-transfer');
  await runQueue();
};

export const cancelTransfer = (): void => {
  // No sentence for it: the bar keeps filling to the end of the chunk, and the button stops working.
  el<HTMLButtonElement>('btn-cancel').disabled = true;
  void cancelFlash();
};

const runQueue = async (): Promise<void> => {
  state.busy = true;
  // Busy until the outcome is on a screen: the Shell disappears while it restarts, and the presence
  // watcher must not read that as a cable being pulled and send the person back to the landing card.
  try {
    // Written before the first byte: if this window dies mid-write, the Shell is still owed these versions.
    await writeRecord();
    for (const [index, item] of state.queue.entries()) {
      if (item.done) continue; // a retry after a failure starts where the queue stopped
      state.step = index + 1;
      state.transfer = { done: 0, total: item.bytes };
      renderTransfer();
      try {
        // The command answers with the configuration the Shell reports once it has restarted, so
        // what comes next is what the device says it runs, not what we meant to write.
        state.config = await flashImage({
          kind: item.kind,
          key: item.key,
          allowVariantSwitch: item.allowVariantSwitch,
          allowDowngrade: item.allowDowngrade,
        });
        item.done = true;
      } catch (error) {
        state.transferError = reason(error);
        break;
      }
    }

    const now = onDeviceVersions(state.config);
    const intended = intendedVersions();
    const behind = kinds.filter(
      (kind) => intended[kind] !== null && now?.[kind] !== intended[kind],
    );

    if (state.transferError) {
      await writeRecord();
      showFailed(state.transferError);
      return;
    }
    if (now && behind.length) {
      showFailed(
        text.failed.cameBack(
          behind
            .map((kind) =>
              text.failed.cameBackItem(kind, now[kind] ?? '', intended[kind] ?? ''),
            )
            .join(' and '),
        ),
      );
      await writeRecord();
      return;
    }

    reportConnected();
    if (!now) {
      // Kept so the next start finishes the check this one could not.
      el('done-summary').textContent = text.done.noVersion;
      el('done-verify').textContent = text.done.replug;
      show('screen-done');
      return;
    }
    await dropRecord();
    el('done-summary').textContent = text.done.versions(now.firmware, now.database);
    el('done-verify').textContent = text.done.checked;
    show('screen-done');
  } finally {
    state.busy = false;
  }
};

/**
 * One screen for anything that stopped a write. Its title is the first sentence of what went wrong
 * - `explain` on the Rust side already chose the words - and whatever followed it stays on the
 * screen as the quiet line, so a long technical answer cannot wrap into a headline.
 */
export const showFailed = (error: string): void => {
  const cut = error.indexOf('. ');
  el('failed-title').textContent = cut < 0 ? (error || text.failed.nothing) : error.slice(0, cut + 1);
  el('failed-detail').textContent = cut < 0 ? '' : error.slice(cut + 2);
  show('screen-failed');
};

export const retryTransfer = async (): Promise<void> => {
  state.transferError = null;
  el<HTMLButtonElement>('btn-cancel').disabled = false;
  show('screen-transfer');
  await runQueue();
};

export const renderTransfer = (): void => {
  const current = state.transfer ?? { done: 0, total: 0 };
  const percent = current.total ? Math.floor((current.done / current.total) * 100) : 0;

  el('transfer-title').textContent = text.transfer.titleBusy;
  el('transfer-percent').textContent = text.transfer.percent(percent);
  el('transfer-bar').style.width = `${percent}%`;
  el('transfer-caption').textContent = text.transfer.keepConnected;

  el('transfer-items').innerHTML = asHtml(
    state.queue.map((item, index) => {
      const moving = index + 1 === state.step && !item.done;
      const marker = item.done
        ? stateWord(text.transfer.item.done, 'latest')
        : moving
          ? spinner()
          : stateWord(text.transfer.item.queued, 'waiting');
      return itemRow(item.label, marker, !item.done && !moving);
    }),
  );
};

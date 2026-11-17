import { show } from '../base/screens.js';
import { scanFrame, scanStart, scanStop } from '../base/bridge.js';
import { human } from '../errors.js';
import type { Scan } from '../types.js';
import { el, reason } from '../utils.js';
import { renderVerify } from './screen.js';
import { say } from './state.js';

let wanted = 0;

export const stopScan = (): void => {
  wanted += 1;
  void scanStop().catch(() => {});
};

export const scanRun = async (device?: string): Promise<void> => {
  stopScan();
  const mine = wanted;
  trouble();
  let started: Scan;
  try {
    started = await scanStart(device);
  } catch (error) {
    trouble(error);
    return;
  }
  choose(started);
  show('screen-scan');
  while (mine === wanted) {
    try {
      const frame = await scanFrame();
      if (mine !== wanted) return;
      await paint(frame);
    } catch (error) {
      if (mine !== wanted) return;
      trouble(error);
      return;
    }
  }
};

const trouble = (error?: unknown): void => {
  say(error === undefined ? null : human(reason(error)));
  renderVerify();
};

const paint = async (jpeg: ArrayBuffer): Promise<void> => {
  const frame = await createImageBitmap(new Blob([jpeg], { type: 'image/jpeg' }));
  const canvas = el<HTMLCanvasElement>('scan-frame');
  canvas.width = frame.width;
  canvas.height = frame.height;
  canvas.getContext('2d')?.drawImage(frame, 0, 0);
  frame.close();
};

const choose = (started: Scan): void => {
  const select = el<HTMLSelectElement>('scan-camera');
  select.replaceChildren(...started.cameras.map((camera) => new Option(camera.label, camera.id)));
  select.value = started.device;
  select.disabled = started.cameras.length < 2;
};

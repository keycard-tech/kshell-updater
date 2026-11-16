import * as text from '../strings.js';
import type { Markup } from '../utils.js';
import { dayDate, html } from '../utils.js';
import type { UnitsStatus } from '../types.js';
import { listLoaded } from '../verify/state.js';

export const listLine = (status: UnitsStatus | null, fetching: boolean): string => {
  if (fetching) return text.verify.fetching;
  if (!listLoaded(status)) return text.verify.noList;
  return status.origin === 'file'
    ? text.verify.fromFile(status.file ?? '')
    : text.verify.fetched(dayDate(status.fetchedAt ?? 0));
};

export const cameraOptions = (
  devices: { deviceId: string; label: string }[],
  current: string,
): Markup =>
  html`${devices.map(
    (device, index) => html`<option value="${device.deviceId}"${device.deviceId === current ? ' selected' : ''}>${device.label || text.camera.unnamed(index + 1)}</option>`,
  )}`;

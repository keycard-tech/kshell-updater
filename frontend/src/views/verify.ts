/* Markup of the verify screen's dynamic lines. */

import * as text from '../strings.js';
import type { Markup } from '../utils.js';
import { dayDate, html } from '../utils.js';
import type { UnitsStatus } from '../types.js';

/** A file name is text, however the list got here. */
export const listLine = (status: UnitsStatus | null, fetching: boolean): Markup => {
  if (fetching) return html`${text.verify.fetching}`;
  if (!status) {
    return html`<span class="ksh__sha-state--bad">${text.verify.noList}</span>${text.verify.noListHint}`;
  }
  const { origin, fetchedAt, file } = status;
  const where =
    origin === 'file'
      ? (file ?? '')
      : text.verify.fetched(dayDate(fetchedAt ?? 0)) +
        (origin === 'cached' ? text.verify.cached : '');
  return html`<span class="ksh__sha--ok">${text.verify.signatureVerified}</span> · ${where}`;
};

export const cameraOptions = (
  devices: { deviceId: string; label: string }[],
  current: string,
): Markup =>
  html`${devices.map(
    (device, index) => html`<option value="${device.deviceId}"${device.deviceId === current ? ' selected' : ''}>${device.label || text.camera.unnamed(index + 1)}</option>`,
  )}`;

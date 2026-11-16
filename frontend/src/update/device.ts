/* The Shell on the cable: finding it, reading it, saying what it reported. */

import { listShells, shellConfiguration } from '../base/bridge.js';
import { show } from '../base/screens.js';
import * as text from '../strings.js';
import type { DeviceInfo } from '../types.js';
import { el, reason } from '../utils.js';
import { state } from '../base/state.js';
import { renderHome } from './home.js';
import { owedVersions, dropRecord } from './record.js';
import { renderRows } from './rows.js';

export const refresh = async (): Promise<void> => {
  try {
    const devices = await listShells();
    if (!devices.length) {
      showWaiting();
      return;
    }
    const device = devices[0];
    if (!device) return;
    state.device = device;
    el('device-name').textContent = text.device.name(
      device.productName || text.device.productName,
    );
    el('status-dot').className = 'ksh__dot';
    el('status-device').textContent = text.device.connected(device.serialNumber ?? '');
    // Plugging in only makes the card pressable; pressing it is what opens the flow.
    show(state.screen);
    await readConfiguration(device);
  } catch (error) {
    el('device-line').textContent = reason(error);
  }
};

export const readConfiguration = async (device: DeviceInfo): Promise<void> => {
  state.config = null;
  state.configError = null;
  try {
    const config = await shellConfiguration(device.path);
    state.config = config;
    const variant = config.fwVariant ?? null;
    const line = el('device-line');
    line.textContent = text.device.serial(device.serialNumber ?? config.serialNumber);
    line.title = text.device.keyTitle(config.publicKey);
    // Default to the variant on the device, so the plain action is not a variant switch.
    if (variant !== null) state.variant.firmware = variant;
    // A remembered update the device now satisfies is finished, whoever wrote the bytes.
    if (state.record && !owedVersions()) void dropRecord();
  } catch (error) {
    state.configError = reason(error);
    el('device-line').textContent = state.configError;
  }
  renderRows();
  renderHome();
};

export const showWaiting = (): void => {
  state.device = null;
  state.config = null;
  // Only the screens that ask something of the device are left behind; verify needs no cable, so a
  // poll that finds nothing must not close it.
  if (state.screen === 'screen-ready' || state.screen === 'screen-transfer') show('screen-home');
  renderHome();
  el('status-device').textContent = text.device.absent;
  el('status-dot').className = 'ksh__dot ksh__dot--off';
};

export const reportConnected = (): void => {
  el('status-device').textContent = text.device.connected(state.device?.serialNumber ?? '');
};

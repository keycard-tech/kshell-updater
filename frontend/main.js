const { invoke, listen } = window.__TAURI__.core
  ? { invoke: window.__TAURI__.core.invoke, listen: window.__TAURI__.event.listen }
  : {};

const el = (id) => document.getElementById(id);

const UI = {
  subtitle: el('subtitle'),
  badge: el('badge'),
  empty: el('card-empty'),
  device: el('card-device'),
  name: el('device-name'),
  serial: el('f-serial'),
  fw: el('f-fw'),
  db: el('f-db'),
  variant: el('f-variant'),
  key: el('f-key'),
  hint: el('device-hint'),
  statusDevice: el('status-device'),
};

function setBadge(text, modifier) {
  UI.badge.textContent = text;
  UI.badge.className = `ksh__badge${modifier ? ` ksh__badge--${modifier}` : ''}`;
}

function showWaiting(message) {
  UI.empty.hidden = false;
  UI.device.hidden = true;
  UI.subtitle.textContent = message;
  UI.statusDevice.textContent = 'No device connected';
  setBadge('No device', 'warn');
}

function showDevice(device) {
  UI.empty.hidden = true;
  UI.device.hidden = false;
  UI.name.textContent = device.productName;
  // The versions replace this as soon as the device answers.
  UI.subtitle.textContent = device.serialNumber ? `serial ${device.serialNumber}` : 'reading…';
  UI.statusDevice.textContent = `${device.productName} connected`;
  setBadge('Device ready', 'ready');
}

const VARIANT = { 0: 'omni — Bitcoin and Ethereum', 1: 'btc-only — Bitcoin' };

async function readConfiguration(device) {
  UI.hint.textContent = 'Reading the app configuration…';
  try {
    const config = await invoke('shell_app_configuration', { path: device.path });
    UI.serial.textContent = device.serialNumber || config.serialNumber;
    UI.fw.textContent = config.fwVersion;
    UI.db.textContent = String(config.dbVersion);
    UI.variant.textContent =
      config.fwVariant === null || config.fwVariant === undefined
        ? 'not reported by this firmware'
        : `${VARIANT[config.fwVariant] ?? config.fwVariant}${config.fwBeta ? ' · beta' : ''}`;
    UI.key.textContent = config.publicKey;
    UI.subtitle.textContent = `on device: firmware ${config.fwVersion}, database ${config.dbVersion}`;
    UI.hint.textContent = 'Read over HID from the device itself.';
  } catch (error) {
    UI.hint.textContent = String(error);
    setBadge('Read failed', 'error');
  }
}

async function refresh() {
  try {
    const devices = await invoke('list_shells');
    if (!devices.length) {
      showWaiting('No Keycard Shell on USB');
      return;
    }
    showDevice(devices[0]);
    await readConfiguration(devices[0]);
  } catch (error) {
    UI.subtitle.textContent = String(error);
    setBadge('USB unavailable', 'error');
  }
}

listen('shell://connected', () => refresh());
listen('shell://disconnected', () => showWaiting('The Shell was unplugged'));

showWaiting('Checking for a connected Shell…');
refresh();

/* Every word the app says, and the pages its links open. */

import type { ImageKind } from './types.js';

export const links = {
  firmwareNotes: 'https://shell.keycard.tech/firmware/release-notes/',
  databaseHistory: 'https://shell.keycard.tech/update/db-version-history/',
  verifyHelp: 'https://keycard.tech/help/verify-keycard-shell-authenticity',
};

export const kinds: { kind: ImageKind; title: string; link: string; url: string }[] = [
  {
    kind: 'firmware',
    title: 'Firmware',
    link: 'Release notes',
    url: links.firmwareNotes,
  },
  {
    kind: 'database',
    title: 'Database',
    link: 'Version history',
    url: links.databaseHistory,
  },
];

/** The two firmware builds, named by the number the device reports. */
export const variants: Record<number, string> = { 0: 'omni', 1: 'btc' };

export const title = (kind: ImageKind): string =>
  kinds.find((entry) => entry.kind === kind)?.title ?? kind;

/** Words the row shows and the code matches on: `cancelled` is `CANCELLED` in src-tauri/src/lib.rs. */
export const status = {
  reading: 'Reading…',
  noFile: 'No file',
  noList: 'No list',
  variantSwitch: 'Variant switch',
  downgrade: 'Downgrade',
  upToDate: 'Up to date',
  newVersion: 'New version',
  cancelled: 'cancelled',
};

export const home = {
  badgeNoDevice: 'USB-C required',
  badgeNotReady: 'Device not ready',
  badgeReady: 'Device ready',
  badgeUpToDate: 'Up to date',
  connect:
    'Unlock your Shell and connect it with a USB-C cable, then choose what to transfer: firmware, database or both.',
  silentDevice: 'The Shell did not answer. Lock it and try again.',
  updated: (parts: string, both: boolean): string =>
    `${parts} on this Shell ${both ? 'can both' : 'can'} be updated. You approve the flash on the device screen.`,
  current: 'This Shell runs the newest firmware and database.',
  start: 'Connect your Shell to start',
  replug: 'Unplug the Shell and plug it back in if it stays like this.',
  owed: (versions: string): string =>
    `An update did not finish: this Shell still runs ${versions}.`,
  offline: 'Offline · updates from file only',
  checking: 'Checking the files…',
  airGapped: 'Works with no internet connection, from files you already downloaded.',
  partFirmware: (version: string): string => `Firmware ${version}`,
  partDatabase: (version: string): string => `database ${version}`,
};

export const summary = {
  selected: (count: number): string =>
    `${count} update${count > 1 ? 's' : ''} selected`,
  none: 'No updates',
  transferring: 'Transferring…',
  transfer: 'Transfer',
  preparing: 'Preparing files…',
  unverified: 'File not verified',
  transferTo: 'Transfer to device',
};

export const row = {
  includeLabel: (title: string): string => `Update ${title}`,
  online: 'Online',
  fromFile: 'From file',
  choose: 'Choose a .bin file',
  fromFileVersion: (version: string, variant: string): string =>
    `${version}${variant ? `-${variant}` : ''} from file`,
  offlineHint: 'Offline - only air-gapped update is available.',
  unreadable: 'The file could not be read.',
  unknownVersion: '—',
  variantOption: (version: string, variant: string): string => `${version} - ${variant}`,
};

export const hash = {
  label: 'sha256',
  unreadable: 'unreadable',
  matchesRelease: 'matches release',
  fromFile: 'from file',
  notVerified: 'not verified',
  checking: 'checking…',
  matchesTitle: 'sha256 matches the published hash',
};

export const transfer = {
  titleBusy: 'Transferring',
  titleCancelled: 'Transfer cancelled',
  titleFailed: 'Update failed',
  step: (step: number, total: number): string => `${step} / ${total}`,
  percent: (percent: number): string => `${percent}%`,
  bytes: (done: string, total: string): string => `${done} / ${total} bytes`,
  keepConnected: 'Please do not disconnect the device.',
  cancelling: 'Cancelling after the chunk in flight…',
  cameBack: (differences: string): string =>
    `The Shell came back with ${differences}.`,
  cameBackItem: (kind: string, now: string, wanted: string): string =>
    `${kind} ${now} instead of ${wanted}`,
  disconnected: 'The device was disconnected.',
  item: {
    done: 'Done',
    failed: 'Failed',
    notStarted: 'Not started',
    queued: 'Queued',
  },
};

export const record = {
  firmware: (version: string): string => `firmware ${version}`,
  database: (version: string): string => `database ${version}`,
};

export const done = {
  versions: (firmware: string, database: string): string =>
    `Your Shell now runs firmware ${firmware} and database ${database}.`,
  checked: 'Checked against the Shell after it restarted.',
  noVersion: 'The transfer finished, but the Shell did not report its version.',
  replug: 'Unplug the Shell and plug it back in to see what it runs now.',
};

export const device = {
  name: (productName: string): string => `${productName} connected`,
  productName: 'Keycard Shell',
  connected: (serial: string): string => `Keycard Shell connected · SN ${serial}`,
  onDevice: (
    serial: string,
    firmware: string,
    variant: string,
    database: string,
  ): string => `SN ${serial} · on device: firmware ${firmware}${variant}, database ${database}`,
  keyTitle: (key: string): string => `attestation key ${key}`,
  absent: 'No device connected',
};

export const releases = {
  offline: 'Offline · updates from file only',
  online: (firmware: string, database: string): string =>
    `Online · latest firmware ${firmware} · latest database ${database}`,
  reading: 'reading the release list…',
};

export const verify = {
  fetching: 'Fetching the produced units list…',
  signatureVerified: 'signature verified',
  fetched: (date: string): string => `fetched ${date}`,
  cached: ', no network since',
  noList: 'No list on this computer',
  noListHint: ' · load one from file, or fetch it once you are online',
  online: 'Online',
  noNetwork: 'No network',
  enableCamera: 'Enable camera',
  fetchingList: 'Fetching units list…',
  loadList: 'Load units list',
};

export const camera = {
  none: (error: string): string =>
    `This computer has no camera the app could open: ${error}`,
  noPictureShown: (error: string): string =>
    `The camera is open but its picture cannot be shown here: ${error}`,
  silent: 'This camera opened but sent no picture. Try another one below.',
  slow: (rate: string): string =>
    `This camera manages about ${rate} pictures a second, too slow to read a code. Try another one below.`,
  unnamed: (index: number): string => `Camera ${index}`,
  readyState: (state: number): string => `readyState ${state}`,
  size: (width: number, height: number): string => `${width}×${height}`,
  noSize: 'no size',
  track: (state: string): string => `track ${state}`,
  trackGone: 'gone',
  counted: (frames: number, seconds: number): string =>
    `${frames} frame${frames === 1 ? '' : 's'} in ${seconds}s`,
};

export interface Shown {
  title: string;
  text: string;
  sub: string;
  button: string;
}

export const verdict = {
  nothingScanned: 'nothing was scanned',
  listUpdated: (date: string): string => `Produced units list last updated ${date}`,
  authentic: (updated: string): Shown => ({
    title: 'Your Shell is authentic',
    text: 'It signed the challenge correctly and it is on our produced units list.',
    sub: `${updated} · checked on this computer`,
    button: 'Done',
  }),
  notRegistered: (uid: string, updated: string): Shown => ({
    title: 'Your Shell is not on our produced units list',
    text: `The response was signed correctly, but no entry matches ${uid}. We can go through the process together.`,
    sub: updated,
    button: 'Learn more',
  }),
  refused: (reason: string, updated: string): Shown => ({
    title: 'Shell was not verified',
    text: `Something went wrong: ${reason}. Go to Settings/Shell/Verify on your Shell and try again.`,
    sub: updated,
    button: 'Try again',
  }),
};

export const dialogs = {
  firmware: 'Choose a firmware image',
  database: 'Choose a database image',
  imageFilter: 'Keycard image',
  units: 'Choose a units list',
  unitsFilter: 'Produced units list',
  extension: 'bin',
};

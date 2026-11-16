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

/** The status word of a row: said, never matched on. */
export const status = {
  reading: 'Reading…',
  noFile: 'No file',
  noList: 'No list',
  variantSwitch: 'Variant switch',
  upToDate: 'Up to date',
  newVersion: 'New version',
  older: 'Older version',
};

export const home = {
  badgeNoDevice: 'USB-C required',
  badgeNotReady: 'Device not ready',
  badgeUpToDate: 'Up to date',
  badgeOwed: 'Update owed',
  badgeUpdates: (count: number): string => `${count} update${count > 1 ? 's' : ''}`,
  connect: 'Connect the Shell with a USB-C cable and unlock it to choose what to write.',
  connectHint: 'Nothing to choose until the Shell is connected',
  prompt: 'Write the newest firmware and the newest Ethereum database to the Shell.',
  runs: (firmware: string, variant: string, database: string): string =>
    `It runs firmware ${firmware}${variant} and database ${database}`,
  silentDevice: 'The Shell did not answer. Lock it and try again.',
  replug: 'Unplug the Shell and plug it back in if it stays like this.',
  owed: (versions: string): string =>
    `An update did not finish: this Shell still runs ${versions}.`,
};

/** The action under the release rows. */

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
  older: (kind: string, onDevice: string): string =>
    `The file you loaded is older than the ${kind} ${onDevice} on this Shell.`,
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
  percent: (percent: number): string => `${percent}%`,
  keepConnected: 'Please do not disconnect the device.',
  // The two words a queue line can hold: the one moving shows the loader, the rest wait their turn.
  item: { done: 'Done', queued: 'Queued' },
};

/**
 * One screen for everything that stopped a write. Its title is the first sentence of what went
 * wrong, so the sentences worth naming live here and the rest arrive from Rust already written.
 */
export const failed = {
  nothing: 'The transfer failed.',
  cameBack: (differences: string): string =>
    `The Shell came back with ${differences}.`,
  cameBackItem: (kind: string, now: string, wanted: string): string =>
    `${kind} ${now} instead of ${wanted}`,
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
  serial: (serial: string): string => `serial ${serial}`,
  keyTitle: (key: string): string => `attestation key ${key}`,
  absent: 'No device connected',
};

export const releases = { offline: 'Offline · updates from file only' };

export const verify = {
  fetching: 'Fetching the produced units list…',
  fetched: (date: string): string => `Units list fetched ${date}`,
  fromFile: (name: string): string => `Units list from ${name}`,
  noList: 'Load one with Own list to verify offline.',
  enableCamera: 'Enable camera',
  fetchingList: 'Fetching units list…',
  loadList: 'Fetch units list',
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
};

export interface Shown {
  title: string;
  text: string;
  sub: string;
  button: string;
}

export const verdict = {
  nothingScanned: 'nothing was scanned',
  authentic: (): Shown => ({
    title: 'Your Shell is authentic',
    text: 'It is in the list of produced units, and it signed this challenge itself.',
    sub: '',
    button: 'Done',
  }),
  notRegistered: (uid: string): Shown => ({
    title: 'No device with this id is in the produced units list',
    text: 'Contact our team to check it.',
    sub: uid,
    button: 'Learn more',
  }),
  refused: (reason: string): Shown => ({
    title: 'This Shell could not be verified',
    text: 'Go to Settings/Shell/Verify on the Shell and try again.',
    sub: reason,
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

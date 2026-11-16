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

export const variants: Record<number, string> = { 0: 'omni', 1: 'btc' };

export const title = (kind: ImageKind): string =>
  kinds.find((entry) => entry.kind === kind)?.title ?? kind;

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
  connect: 'Connect your Shell with a USB-C cable and unlock it to upgrade firmware / database.',
  connectHint: 'Nothing to choose until the Shell is connected',
  prompt: 'Write the newest firmware and the newest Ethereum database to the Shell.',
  runs: (firmware: string, variant: string, database: string): string =>
    `Firmware - ${firmware}${variant}, Database - ${database}`,
  silentDevice: 'The Shell did not answer. Unlock it and try again.',
  replug: 'Unplug the Shell and plug it back in.',
  owed: (versions: string): string => `Update failed: Shell still runs ${versions}.`,
};

export const summary = {
  selected: (count: number): string => `${count} update${count > 1 ? 's' : ''} selected`,
  none: 'No updates',
  transferring: 'Transferring…',
  transfer: 'Transfer',
  preparing: 'Preparing files…',
  unverified: 'File not verified',
  transferTo: 'Transfer to device',
};

export const row = {
  choose: 'Choose a .bin file',
  fromFileVersion: (version: string, variant: string): string =>
    `${version}${variant ? `-${variant}` : ''} from file`,
  older: (kind: string, onDevice: string): string =>
    `The file you loaded is older than the ${kind} ${onDevice} on your Shell.`,
  unreadable: 'File could not be read.',
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
  item: { done: 'Done', queued: 'Queued' },
};

export const failed = {
  nothing: 'The transfer failed.',
  cameBack: (differences: string): string => `Shell came back with ${differences}.`,
  cameBackItem: (kind: string, now: string, wanted: string): string =>
    `${kind} ${now} instead of ${wanted}`,
};

export const record = {
  firmware: (version: string): string => `firmware ${version}`,
  database: (version: string): string => `database ${version}`,
};

export const done = {
  versions: (firmware: string, database: string): string =>
    `Shell now runs firmware ${firmware} and database ${database}.`,
};

export const device = {
  name: (productName: string): string => `${productName} connected`,
  productName: 'Keycard Shell',
  connected: (serial: string): string => `Keycard Shell connected · SN ${serial}`,
  onDevice: (firmware: string, variant: string, database: string): string =>
    `Installed on Shell · firmware ${firmware}${variant} · database ${database}`,
  keyTitle: (key: string): string => `attestation key ${key}`,
  absent: 'No device connected',
};

export const verify = {
  fetching: 'Fetching the produced units list…',
  fetched: (date: string): string => `Units list fetched ${date}`,
  fromFile: (name: string): string => `Units list from ${name}`,
  noList: 'Load own list to verify offline.',
  enableCamera: 'Enable camera',
  fetchingList: 'Fetching units list…',
  loadList: 'Fetch units list',
};

export const camera = {
  silent: 'Selected camera opened but sent no picture. Try another one below.',
  slow: (rate: string): string =>
    `Selected camera manages about ${rate} pictures a second, too slow to read a code. Try another one below.`,
  stopped: 'This camera stopped by itself. Pick another one below.',
  didNotWork: 'did not open',
  sentNothing: 'sent no picture',
  unnamed: (index: number): string => `Camera ${index}`,
};

export interface Shown {
  title: string;
  text: string;
  sub?: string;
  button: string;
}

export const verdict = {
  nothingScanned: 'nothing was scanned',
  authentic: (): Shown => ({
    title: 'Your Shell is authentic',
    text: 'The verification is completed successfully. Your Shell is in the list of produced units.',
    button: 'Done',
  }),
  notRegistered: (uid: string): Shown => ({
    title: `No device ${uid} is in the produced units list`,
    text: 'Contact our team to check it.',
    sub: uid,
    button: 'Learn more',
  }),
  refused: (reason: string): Shown => ({
    title: 'Shell could not be verified',
    text: 'Go to Settings/Shell/Verify on your Shell and try again.',
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

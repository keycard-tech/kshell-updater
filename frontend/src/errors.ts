type Fault = 'offline' | 'damaged' | 'qr' | 'device' | 'camera' | 'unexpected';

export const sentence: Record<Fault, string> = {
  offline: 'Offline - no list available, connection error',
  damaged: 'That file did not arrive whole - try again',
  qr: 'Invalid QR',
  device: 'The Shell stopped answering - plug it in again',
  camera: 'The camera did not start - try another one',
  unexpected: 'Something went wrong - try again',
};

const says = (detail: string, words: RegExp): boolean => words.test(detail);

const CAMERA_WORDS = /camera|videoinput|notallowed|permission|deviceId|picture/i;

const LIST_BLAMED = /units list|produced units|list file/i;

const FAULT_WORDS: [Fault, RegExp][] = [
  ['damaged', /was refused|checksum|sha256|hash|too large|truncat|lowercase hex|expected|decode/i],
  ['qr', /qr|signature|digest|pinned|challenge|device answer|unit id|recovers/i],
  ['device', /unplug|usb|hidraw|\bhid\b|disappear|disconnect|no device|not connected|pipe/i],
  [
    'offline',
    /could not reach|connect|proxy|tunnel|resolve|timed out|timeout|allowance|rate limit|responded|network|offline|403|429|5\d\d/i,
  ],
];

const faultOf = (detail: string, about?: 'answer'): Fault => {
  if (says(detail, CAMERA_WORDS)) return 'camera';
  if (about === 'answer' && !says(detail, LIST_BLAMED)) return 'qr';
  return FAULT_WORDS.find(([, words]) => says(detail, words))?.[0] ?? 'unexpected';
};

const writtenForAPerson = (detail: string): boolean =>
  /^[A-Z].*[.!]$/.test(detail) &&
  !/https?:|\bos error\b|\berrno\b|\bE[A-Z]{4,6}\b|tunnel|connection refused|\d{1,3}(\.\d{1,3}){3}|:\s/i.test(
    detail,
  );

export const human = (detail: string, about?: 'answer'): string =>
  writtenForAPerson(detail) ? detail : sentence[faultOf(detail, about)];

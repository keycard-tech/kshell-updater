type Fault = 'offline' | 'damaged' | 'qr' | 'device' | 'camera' | 'unexpected';

export const sentence: Record<Fault, string> = {
  offline: 'Offline - no list available, connection error',
  damaged: 'That file did not arrive whole - try again',
  qr: 'Invalid QR',
  device: 'The Shell stopped answering - plug it in again',
  camera: 'The camera did not start - try another one',
  unexpected: 'Something went wrong - try again',
};

const faultOf = (detail: string, about?: 'answer'): Fault => {
  const says = (words: RegExp): boolean => words.test(detail);
  if (says(/camera|videoinput|notallowed|permission|deviceId|picture/i)) return 'camera';
  // A refusal of a scanned answer is about the code, unless it names the list - that is a file.
  if (about === 'answer' && !says(/units list|produced units|list file/i)) return 'qr';
  if (says(/was refused|checksum|sha256|hash|too large|truncat|lowercase hex|expected|decode/i))
    return 'damaged';
  if (says(/qr|signature|digest|pinned|challenge|device answer|unit id|recovers/i)) return 'qr';
  if (says(/unplug|usb|hidraw|\bhid\b|disappear|disconnect|no device|not connected|pipe/i))
    return 'device';
  if (
    says(
      /could not reach|connect|proxy|tunnel|resolve|timed out|timeout|allowance|rate limit|responded|network|offline|403|429|5\d\d/i,
    )
  )
    return 'offline';
  return 'unexpected';
};

const writtenForAPerson = (detail: string): boolean =>
  /^[A-Z].*[.!]$/.test(detail) &&
  !/https?:|\bos error\b|\berrno\b|\bE[A-Z]{4,6}\b|tunnel|connection refused|\d{1,3}(\.\d{1,3}){3}|:\s/i.test(
    detail,
  );

export const human = (detail: string, about?: 'answer'): string =>
  writtenForAPerson(detail) ? detail : sentence[faultOf(detail, about)];

import * as text from '../strings.js';
import { dayDate } from '../utils.js';
import type { UnitsStatus } from '../types.js';
import { listLoaded } from '../verify/state.js';

export const listLine = (status: UnitsStatus | null, fetching: boolean): string => {
  if (fetching) return text.verify.fetching;
  if (!listLoaded(status)) return text.verify.noList;
  return status.origin === 'file'
    ? text.verify.fromFile(status.file ?? '')
    : text.verify.fetched(dayDate(status.fetchedAt ?? 0));
};

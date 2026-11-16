/* Everything the app remembers between renders. */

import type {
  AppConfiguration,
  DeviceInfo,
  ImageInfo,
  ImageKind,
  ReleaseFile,
  UpdateRecord,
} from '../types.js';
import type { Screen } from './screens.js';

export type Mode = 'online' | 'file';

export type ByKind<T> = Record<ImageKind, T>;

const byKind = <T>(value: T): ByKind<T> => ({ firmware: value, database: value });

/** What a row has read from disk: the image it would write, or why it could not. */
export interface ReadyFile {
  info?: ImageInfo;
  key?: string;
  error?: string;
}

export interface QueueItem {
  kind: ImageKind;
  /** `Prepared.key`: the release URL, or `file://<path>` for a file from disk. */
  key: string;
  label: string;
  bytes: number;
  done: boolean;
  allowVariantSwitch: boolean;
  allowDowngrade: boolean;
}

export interface State {
  device: DeviceInfo | null;
  config: AppConfiguration | null;
  /** Only the newest release of each kind, so nothing here can offer an older build. */
  files: ByKind<ReleaseFile[]>;
  mode: ByKind<Mode>;
  include: ByKind<boolean>;
  variant: ByKind<number | null>;
  ready: ByKind<ReadyFile | null>;
  listError: string | null;
  busy: boolean;
  scanning: boolean;
  /** Keyed by `Prepared.key`: whether the file behind it passed its checks. */
  cache: Record<string, { ok: boolean; note?: string }>;
  prepared: Record<string, ImageInfo>;
  preloading: boolean;
  queue: QueueItem[];
  step: number;
  transfer: { done: number; total: number } | null;
  transferError: string | null;
  configError: string | null;
  record: UpdateRecord | null;
  screen: Screen;
}

export const state: State = {
  device: null,
  config: null,
  files: byKind([]),
  mode: byKind('online'),
  include: byKind(true),
  variant: { firmware: 0, database: null },
  ready: byKind(null),
  listError: null,
  busy: false,
  scanning: false,
  cache: {},
  prepared: {},
  preloading: true,
  queue: [],
  step: 0,
  transfer: null,
  transferError: null,
  configError: null,
  record: null,
  screen: 'screen-home',
};

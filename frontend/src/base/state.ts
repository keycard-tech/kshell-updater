import type {
  AppConfiguration,
  DeviceInfo,
  ImageInfo,
  ImageKind,
  ReleaseFile,
  UpdateRecord,
} from '../types.js';
import type { Screen } from './screens.js';

type Mode = 'online' | 'file';

export type ByKind<T> = Record<ImageKind, T>;

const byKind = <T>(value: T): ByKind<T> => ({ firmware: value, database: value });

export interface ReadyFile {
  info?: ImageInfo;
  key?: string;
  error?: string;
}

interface QueueItem {
  kind: ImageKind;
  key: string;
  label: string;
  bytes: number;
  done: boolean;
}

interface State {
  device: DeviceInfo | null;
  config: AppConfiguration | null;
  files: ByKind<ReleaseFile[]>;
  mode: ByKind<Mode>;
  include: ByKind<boolean>;
  variant: ByKind<number | null>;
  ready: ByKind<ReadyFile | null>;
  listError: string | null;
  offline: boolean;
  busy: boolean;
  scanning: boolean;
  cache: Record<string, { ok: boolean; note?: string }>;
  prepared: Record<string, ImageInfo>;
  preloading: boolean;
  queue: QueueItem[];
  step: number;
  transfer: { done: number; total: number } | null;
  transferError: string | null;
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
  offline: false,
  busy: false,
  scanning: false,
  cache: {},
  prepared: {},
  preloading: true,
  queue: [],
  step: 0,
  transfer: null,
  transferError: null,
  record: null,
  screen: 'screen-home',
};

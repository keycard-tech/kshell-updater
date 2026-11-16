import {
  clearUpdateRecord,
  loadUpdateRecord,
  saveUpdateRecord,
} from '../base/bridge.js';
import { state } from '../base/state.js';
import * as text from '../strings.js';
import type { AppConfiguration, ImageKind, UpdateRecord } from '../types.js';

type Versions = Record<ImageKind, string | null>;

type DeviceVersions = Record<ImageKind, string>;

export const onDeviceVersions = (config: AppConfiguration | null): DeviceVersions | null =>
  config
    ? { firmware: config.fwVersion, database: String(config.dbVersion) }
    : null;

export const intendedVersions = (): Versions => {
  const want: Versions = { firmware: null, database: null };
  for (const item of state.queue) {
    const version = state.prepared[item.key]?.version;
    if (version !== undefined) want[item.kind] = String(version);
  }
  return want;
};

export const owedVersions = (): string[] | null => {
  const record = state.record;
  if (!record || !state.config) return null;
  const serial = state.config.serialNumber ?? state.device?.serialNumber ?? '';
  if (record.serial && serial && record.serial !== serial) return null;
  const now = onDeviceVersions(state.config);
  if (!now) return null;
  const behind = (['firmware', 'database'] as ImageKind[]).filter(
    (kind) => record[kind] && record[kind] !== now[kind],
  );
  return behind.length
    ? behind.map((kind) => text.record[kind](now[kind] ?? ''))
    : null;
};

export const writeRecord = async (): Promise<void> => {
  const intended = intendedVersions();
  const record: UpdateRecord = {
    serial: state.config?.serialNumber ?? state.device?.serialNumber ?? '',
    firmware: intended.firmware,
    database: intended.database,
  };
  state.record = record;
  await saveUpdateRecord(record).catch(() => {});
};

export const dropRecord = async (): Promise<void> => {
  state.record = null;
  await clearUpdateRecord().catch(() => {});
};

export const readRecord = async (): Promise<void> => {
  state.record = (await loadUpdateRecord().catch(() => null)) ?? null;
};

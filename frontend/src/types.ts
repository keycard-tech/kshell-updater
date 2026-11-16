/* What crosses the Rust bridge. */

export type ImageKind = 'firmware' | 'database';

export interface ReleaseFile {
  name: string;
  url: string;
  date: string;
  version: string | null;
  /** 0 omni, 1 btc-only: the same number the device reports in `fwVariant`. */
  variant: number | null;
  sha256: string | null;
}

export interface Catalogue {
  files: ReleaseFile[];
  unofferable: number;
  verified: boolean;
  note: string | null;
}

export interface ImageInfo {
  version: string;
  kind: ImageKind;
  fwVariant: number | null;
  bytes: number;
  sha256: string;
  matchesRelease: string | null;
}

export interface Prepared {
  info: ImageInfo;
  key: string;
}

export interface DeviceInfo {
  productName: string;
  usbProductId: number;
  path: string;
  serialNumber: string | null;
}

export interface AppConfiguration {
  fwVersion: string;
  dbVersion: number;
  serialNumber: string;
  publicKey: string;
  fwVariant: number | null;
  fwBeta: boolean | null;
}

export interface UpdateRecord {
  serial: string;
  firmware: string | null;
  database: string | null;
}

export interface FlashRequest {
  kind: ImageKind;
  key: string;
  allowVariantSwitch: boolean;
  allowDowngrade: boolean;
}

export interface FlashProgress {
  kind: ImageKind;
  done: number;
  total: number;
}

export interface Preloaded {
  key: string;
  ok: boolean;
  note: string | null;
}

export interface UnitsStatus {
  build: number | null;
  origin: string | null;
  fetchedAt: number | null;
  file: string | null;
}

export interface Challenge {
  text: string;
  svg: string;
}

export type Verdict =
  | { outcome: 'authentic' }
  | { outcome: 'notRegistered'; uid: string }
  | { outcome: 'refused'; reason: string };

import type {
  AppConfiguration,
  Catalogue,
  Challenge,
  Scan,
  DeviceInfo,
  FlashRequest,
  ImageKind,
  Prepared,
  UnitsStatus,
  UpdateRecord,
  Verdict,
} from '../types.js';

const call = <T = undefined>(command: string, args?: Record<string, unknown>): Promise<T> =>
  window.__TAURI__.core.invoke(command, args) as Promise<T>;

export const listShells = (): Promise<DeviceInfo[]> => call('list_shells');

export const networkStatus = (): Promise<boolean> => call('network_status');

export const shellConfiguration = (path: string): Promise<AppConfiguration> =>
  call('shell_app_configuration', { path });

export const listReleases = (kind: ImageKind): Promise<Catalogue> =>
  call('list_releases', { kind });

export const preloadImages = (
  files: { kind: ImageKind; url: string }[],
): Promise<Prepared[]> => call('preload_images', { files });

export const pickImageFile = (kind: ImageKind, path: string): Promise<Prepared> =>
  call('pick_image_file', { file: { kind, path } });

export const flashImage = (request: FlashRequest): Promise<AppConfiguration> =>
  call('flash_image', { request });

export const cancelFlash = (): Promise<undefined> => call('cancel_flash');

export const saveUpdateRecord = (record: UpdateRecord): Promise<undefined> =>
  call('save_update_record', { record });

export const loadUpdateRecord = (): Promise<UpdateRecord | null> =>
  call('load_update_record');

export const clearUpdateRecord = (): Promise<undefined> => call('clear_update_record');

export const unitsStatus = (): Promise<UnitsStatus> => call('units_status');

export const unitsFetch = (): Promise<UnitsStatus> => call('units_fetch');

export const unitsUseFile = (path: string): Promise<UnitsStatus> =>
  call('units_use_file', { path });

export const unitsChallenge = (renew: boolean): Promise<Challenge> =>
  call('units_challenge', { renew });

export const unitsCheck = (text: string): Promise<Verdict> => call('units_check', { text });

export const linkCode = (url: string): Promise<string> => call('link_code', { url });

export const scanStart = (device?: string): Promise<Scan> => call('scan_start', { device });

export const scanStop = (): Promise<undefined> => call('scan_stop');

export const scanFrame = (): Promise<ArrayBuffer> =>
  window.__TAURI__.core.invoke('scan_frame', {}, { responseType: 'arraybuffer' }) as Promise<
    ArrayBuffer
  >;

export const chooseFile = (options: {
  title: string;
  filterName: string;
  extensions: string[];
}): Promise<string | null> =>
  call<string | null>('plugin:dialog|open', {
    options: {
      multiple: false,
      title: options.title,
      filters: [{ name: options.filterName, extensions: options.extensions }],
    },
  });

export const openExternal = (url: string): Promise<undefined> =>
  call('plugin:opener|open_url', { url });

export const events = {
  reachable: 'net://reachable',
  connected: 'shell://connected',
  disconnected: 'shell://disconnected',
  flashProgress: 'shell://flash-progress',
  preloaded: 'shell://preloaded',
  scanned: 'scan://code',
};

export const listen = <T>(event: string, handler: (payload: T) => void): void => {
  void window.__TAURI__.event.listen<T>(event, (message) => handler(message.payload));
};

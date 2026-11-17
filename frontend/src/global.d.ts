interface TauriBridge {
  core: {
    invoke(
      command: string,
      args?: Record<string, unknown>,
      options?: { responseType?: 'json' | 'arraybuffer' },
    ): Promise<unknown>;
  };
  event: {
    listen<T>(name: string, handler: (event: { payload: T }) => void): Promise<() => void>;
  };
}

interface Window {
  __TAURI__: TauriBridge;
  kshell?: {
    scanAnswer: (code: string) => Promise<void>;
    renderScan: () => void;
    verify: import('./verify/state.js').VerifyState;
  };
}

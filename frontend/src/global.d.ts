/* What the webview and the vendored decoder put on `window`. */

interface TauriBridge {
  core: {
    invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  };
  event: {
    listen<T>(name: string, handler: (event: { payload: T }) => void): Promise<() => void>;
  };
}

interface DecodedCode {
  data: string;
}

interface Window {
  __TAURI__: TauriBridge;
  jsQR?: (
    data: Uint8ClampedArray,
    width: number,
    height: number,
    options?: { inversionAttempts?: 'dontInvert' | 'attemptBoth' },
  ) => DecodedCode | null;
  /** The one global on purpose: the screenshot harness in shots/ drives the verify flow through it. */
  kshell?: {
    scanAnswer: (code: string) => Promise<void>;
    renderScan: () => void;
    verify: import('./verify/state.js').VerifyState;
  };
}

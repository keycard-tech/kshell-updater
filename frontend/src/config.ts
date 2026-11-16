/* What the app is set to: numbers, paths and the camera it asks for. */

export const images = {
  shell: 'img/shell.png',
  connect: 'img/connect_shell.png',
  success: 'img/success.svg',
  untrustworthy: 'img/untrustworthy.svg',
  failed: 'img/fail.svg',
};

export const presencePollMs = 1500;

/** A flashed firmware restarts the Shell: how long to keep asking it for its configuration. */
export const deviceReturn = { attempts: 20, delayMs: 1000 };

/** A "5M" webcam left alone picks 2592x1944 over USB and delivers seconds per frame; ideal, not exact, because a constraint the camera cannot meet means no camera at all. */
export const cameraMode: MediaTrackConstraints = {
  width: { ideal: 640 },
  height: { ideal: 480 },
  frameRate: { ideal: 30 },
};

/** Enough for a code on a device screen, and what every frame costs. */
export const decodeWindowPx = 420;

export const scanIntervalMs = 120;

/** A camera that opens and sends nothing: re-ask once, then say so. */
export const silentCamera = { firstWaitMs: 3000, secondWaitMs: 4000 };

/** Pictures a second below which no code can be read, measured over a window. */
export const slowCamera = { windowMs: 2000, minFrames: 2, minFps: 3 };

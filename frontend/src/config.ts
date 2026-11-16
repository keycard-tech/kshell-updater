export const images = {
  shell: 'img/shell.png',
  connect: 'img/connect_shell.png',
  success: 'img/success.svg',
  untrustworthy: 'img/untrustworthy.svg',
  failed: 'img/fail.svg',
};

export const presencePollMs = 1500;

export const cameraMode: MediaTrackConstraints = {
  width: { ideal: 640 },
  height: { ideal: 480 },
  frameRate: { ideal: 30 },
};

export const decodeWindowPx = 420;

export const scanIntervalMs = 120;

export const silentCamera = { firstWaitMs: 3000, secondWaitMs: 4000 };

export const slowCamera = { windowMs: 2000, minFrames: 2, minFps: 3 };

import { show } from '../base/screens.js';
import { state } from '../base/state.js';
import { cameraMode, decodeWindowPx, scanIntervalMs, silentCamera, slowCamera } from '../config.js';
import * as text from '../strings.js';
import { asHtml, el, reason } from '../utils.js';
import { cameraOptions } from '../views/verify.js';
import { renderVerify } from './screen.js';
import { renderScan, say, verify } from './state.js';

interface FrameCallbackVideo {
  requestVideoFrameCallback?: (callback: () => void) => number;
  cancelVideoFrameCallback?: (handle: number) => void;
}

export const stopCamera = (): void => {
  if (verify.silentWait !== null) clearTimeout(verify.silentWait);
  verify.silentWait = null;
  verify.stopWatch?.();
  verify.stopWatch = null;
  verify.frames = 0;
  verify.slowSaid = false;
  if (verify.decodeTimer !== null) clearInterval(verify.decodeTimer);
  verify.decodeTimer = null;
  for (const track of verify.stream?.getTracks() ?? []) track.stop();
  verify.stream = null;
  el<HTMLVideoElement>('scan-video').srcObject = null;
};

export const cameraStart = async (
  onCode: (text: string) => void,
  deviceId?: string,
): Promise<void> => {
  const generation = ++verify.generation;
  try {
    verify.stream = await navigator.mediaDevices.getUserMedia({
      video: { ...cameraMode, ...(deviceId ? { deviceId: { exact: deviceId } } : {}) },
      audio: false,
    });
  } catch (error) {
    verify.problem = text.camera.none(reason(error));
    renderVerify();
    return;
  }
  verify.problem = null;
  renderScan();
  show('screen-scan');

  const video = el<HTMLVideoElement>('scan-video');
  video.srcObject = verify.stream;
  video.play().catch((error: unknown) => {
    if (generation !== verify.generation || (error as DOMException)?.name === 'AbortError') return;
    verify.problem = text.camera.noPictureShown(reason(error));
    renderScan();
  });
  await listCameras();
  startDecoding(onCode);
};

const listCameras = async (): Promise<void> => {
  const devices = (await navigator.mediaDevices.enumerateDevices().catch(() => []))
    .filter((device) => device.kind === 'videoinput')
    .map((device) => ({ deviceId: device.deviceId, label: device.label }));
  const track = verify.stream?.getVideoTracks()[0];
  // The same device id is spelled with braces on some platforms and without them on others.
  const current = String(track?.getSettings().deviceId ?? '')
    .replace(/^\{/, '')
    .replace(/\}$/, '');
  const select = el<HTMLSelectElement>('scan-camera');
  select.innerHTML = asHtml(cameraOptions(devices, current));
  select.disabled = devices.length < 2;
};

const startDecoding = (onCode: (text: string) => void): void => {
  const video = el<HTMLVideoElement>('scan-video');
  const canvas = document.createElement('canvas');
  canvas.width = decodeWindowPx;
  canvas.height = decodeWindowPx;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) return;

  if (verify.silentWait !== null) clearTimeout(verify.silentWait);
  if (verify.decodeTimer !== null) clearInterval(verify.decodeTimer);
  verify.silentSaid = false;
  verify.silentWait = setTimeout(() => {
    if (video.readyState >= 2 || state.screen !== 'screen-scan') return;
    video.srcObject = verify.stream;
    video.play().catch(() => {});
    verify.silentWait = setTimeout(() => {
      if (video.readyState >= 2 || state.screen !== 'screen-scan') return;
      verify.silentSaid = true;
      say(text.camera.silent);
      console.warn('camera sent no picture:', videoState(video));
    }, silentCamera.secondWaitMs);
  }, silentCamera.firstWaitMs);

  verify.frames = 0;
  verify.openedAt = performance.now();
  verify.rate = { at: performance.now(), frames: 0 };
  verify.stopWatch?.();
  verify.stopWatch = watchFrames(video, () => {
    verify.frames += 1;
  });

  verify.decodeTimer = setInterval(() => {
    if (state.screen !== 'screen-scan' || video.readyState < 2) return;

    if (verify.silentSaid) {
      verify.silentSaid = false;
      say(null);
    }
  
    if (performance.now() - verify.rate.at > slowCamera.windowMs) {
      const seconds = (performance.now() - verify.rate.at) / 1000;
      const fps = (verify.frames - verify.rate.frames) / seconds;
      verify.rate = { at: performance.now(), frames: verify.frames };
      const slow = verify.frames >= slowCamera.minFrames && fps < slowCamera.minFps;
      if (slow !== verify.slowSaid) {
        verify.slowSaid = slow;
        say(slow ? text.camera.slow(fps.toFixed(1)) : null);
        if (slow) console.warn('camera is too slow to scan:', videoState(video));
      }
    }
    context.drawImage(video, 0, 0, decodeWindowPx, decodeWindowPx);
    const code = window.jsQR?.(
      context.getImageData(0, 0, decodeWindowPx, decodeWindowPx).data,
      decodeWindowPx,
      decodeWindowPx,
      { inversionAttempts: 'dontInvert' },
    );
    if (code?.data) onCode(code.data);
  }, scanIntervalMs);
};

const videoState = (video: HTMLVideoElement): string => {
  const track = verify.stream?.getVideoTracks()[0];
  return JSON.stringify({
    deviceId: track?.getSettings().deviceId ?? null,
    resolution: video.videoWidth ? `${video.videoWidth}x${video.videoHeight}` : null,
    settings: track?.getSettings() ?? null,
    readyState: video.readyState,
    trackState: track?.readyState ?? 'gone',
    frames: verify.frames,
    seconds: Math.round((performance.now() - verify.openedAt) / 1000),
    error: video.error ? String(video.error) : null,
  });
};

const watchFrames = (video: HTMLVideoElement, onFrame: () => void): (() => void) | null => {
  const element = video as unknown as FrameCallbackVideo;
  if (element.requestVideoFrameCallback) {
    let handle = 0;
    const loop = (): void => {
      onFrame();
      handle = element.requestVideoFrameCallback?.(loop) ?? handle;
    };
    handle = element.requestVideoFrameCallback(loop);
    return () => element.cancelVideoFrameCallback?.(handle);
  }
  const track = verify.stream?.getVideoTracks()[0] as EventTarget | undefined;
  if (!track) return null;
  track.addEventListener('frame', onFrame);
  return () => track.removeEventListener('frame', onFrame);
};

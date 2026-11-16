import { show } from '../base/screens.js';
import { state } from '../base/state.js';
import {
  cameraRetryPauseMs,
  decodeWindowPx,
  scanIntervalMs,
  silentCamera,
  slowCamera,
} from '../config.js';
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

/** WebKit satisfies a request as a whole or not at all: a size a 4K-only webcam does not offer, or a
    frame rate the driver does not publish, is not ignored but means no camera at all - and the failed
    opens are what wedge PipeWire, after which every attempt times out. Nothing is asked for but the
    device picked, and the decode window is a centred square crop, so any mode reads. A smaller mode is
    not chased either: on a UVC webcam the low resolutions are uncompressed and give fewer frames over
    USB 2 than the high-resolution MJPEG the driver defaults to. The chosen camera is asked for twice
    before loosening to any camera, because the refusal here is usually a device still settling. */
const cameraAttempts = (deviceId?: string): MediaTrackConstraints[] =>
  deviceId ? [{ deviceId: { exact: deviceId } }, { deviceId: { exact: deviceId } }, {}] : [{}];

const wait = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

const liveDeviceId = (): string =>
  String(verify.stream?.getVideoTracks()[0]?.getSettings().deviceId ?? '');

const markBroken = (deviceId: string, why: string): void => {
  if (!deviceId) return;
  verify.broken.set(deviceId, why);
  void listCameras();
};

const openCamera = async (
  deviceId: string | undefined,
  generation: number,
): Promise<MediaStream | null> => {
  const attempts = cameraAttempts(deviceId);
  let last: unknown = new Error('no camera offered');
  for (const [index, video] of attempts.entries()) {
    // A camera that refused once usually opens a moment later; hammering it is how PipeWire gets
    // wedged, and a wedged PipeWire answers 'Timeout was reached' for every camera after it.
    if (index) await wait(cameraRetryPauseMs * index);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
      if (generation !== verify.generation) {
        for (const track of stream.getTracks()) track.stop();
        return null;
      }
      if (deviceId) verify.broken.delete(deviceId);
      if (index > 0) {
        console.warn(
          'the chosen camera did not open, any camera did:',
          JSON.stringify({
            asked: attempts[0],
            settings: stream.getVideoTracks()[0]?.getSettings() ?? null,
          }),
        );
      }
      return stream;
    } catch (error) {
      last = error;
      if (deviceId) markBroken(deviceId, 'open');
      console.warn('camera attempt refused:', JSON.stringify({ video, error: reason(error) }));
    }
  }
  throw last;
};

export const cameraStart = async (
  onCode: (text: string) => void,
  deviceId?: string,
): Promise<void> => {
  // One stream at a time: a track left open keeps the device claimed, and the next attempt then
  // finds the camera busy and the device list grown by a phantom.
  stopCamera();
  const generation = ++verify.generation;
  let stream: MediaStream | null;
  try {
    stream = await openCamera(deviceId, generation);
  } catch (error) {
    verify.problem = text.camera.none(reason(error));
    renderVerify();
    return;
  }
  if (!stream) return;
  verify.stream = stream;
  verify.problem = null;
  renderScan();
  show('screen-scan');

  const video = el<HTMLVideoElement>('scan-video');
  video.srcObject = verify.stream;
  // An IR camera on a laptop vanishes from the system while the app holds it: the track ends and the
  // preview freezes on a frame that will never change.
  stream.getVideoTracks()[0]?.addEventListener('ended', () => {
    if (generation !== verify.generation) return;
    verify.problem = text.camera.stopped;
    renderScan();
    markBroken(liveDeviceId(), 'open');
  });
  video.play().catch((error: unknown) => {
    if (generation !== verify.generation || (error as DOMException)?.name === 'AbortError') return;
    verify.problem = text.camera.noPictureShown(reason(error));
    renderScan();
  });
  await listCameras();
  startDecoding(onCode);
};

const listCameras = async (): Promise<void> => {
  const seen = (await navigator.mediaDevices.enumerateDevices().catch(() => []))
    .filter((device) => device.kind === 'videoinput')
    .map((device) => ({ deviceId: device.deviceId, label: device.label }));
  const track = verify.stream?.getVideoTracks()[0];
  // The same device id is spelled with braces on some platforms and without them on others.
  const current = String(track?.getSettings().deviceId ?? '')
    .replace(/^\{/, '')
    .replace(/\}$/, '');
  const select = el<HTMLSelectElement>('scan-camera');
  const choices = order(withoutRepeats(seen, current), current);
  select.innerHTML = asHtml(cameraOptions(choices, current));
  select.disabled = choices.length < 2;
};

/** One camera that refused to open or sent nothing is worth naming in the list, where the person can
    see why it is at the bottom, rather than meeting it again as a mystery. */
const order = (
  devices: { deviceId: string; label: string }[],
  current: string,
): { deviceId: string; label: string }[] =>
  devices
    .map((device, index) => {
      const why = verify.broken.get(device.deviceId);
      const named = device.label || text.camera.unnamed(index + 1);
      return {
        deviceId: device.deviceId,
        label: why
          ? `${named} — ${why === 'picture' ? text.camera.sentNothing : text.camera.didNotWork}`
          : named,
        rank: device.deviceId === current ? 0 : why ? 2 : 1,
      };
    })
    .sort((left, right) => left.rank - right.rank);

let lastFrameDrawn = -1;

/** PipeWire and v4l2 report one physical camera several times, each with its own id; a list of
    repeats is unreadable and every id but one opens a device that is already claimed. */
const withoutRepeats = (
  devices: { deviceId: string; label: string }[],
  current: string,
): { deviceId: string; label: string }[] => {
  const distinct = new Map<string, { deviceId: string; label: string }>();
  for (const device of devices) {
    const name = device.label || device.deviceId;
    const held = distinct.get(name);
    if (!held || (device.deviceId === current && held.deviceId !== current)) {
      distinct.set(name, device);
    }
  }
  return [...distinct.values()];
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
      markBroken(liveDeviceId(), 'picture');
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
    // Drawing a video whose track has died is enough to take WebKitGTK down with it.
    const track = verify.stream?.getVideoTracks()[0];
    if (
      state.screen !== 'screen-scan' ||
      document.hidden ||
      !track ||
      track.readyState !== 'live' ||
      video.readyState < 2
    ) {
      return;
    }

    if (verify.silentSaid) {
      verify.silentSaid = false;
      say(null);
    }
  
    const nothingNewArrived = verify.frames > 0 && verify.frames === lastFrameDrawn;
    if (nothingNewArrived) return;
    lastFrameDrawn = verify.frames;
    const square = Math.min(video.videoWidth, video.videoHeight);
    if (!square) return;

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
    context.drawImage(
      video,
      (video.videoWidth - square) / 2,
      (video.videoHeight - square) / 2,
      square,
      square,
      0,
      0,
      decodeWindowPx,
      decodeWindowPx,
    );
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

import argparse
import glob
import gi
import importlib.util
import json
import os
import shutil
import subprocess
import sys

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location('webkit_check', f'{HERE}/webkit-check.py')
harness = importlib.util.module_from_spec(spec)
ARGV = sys.argv[1:]
sys.argv = [spec.origin]  # the module reads argv only in main(), which is guarded
spec.loader.exec_module(harness)

PROBE = """
  (() => {
    const video = document.getElementById('scan-video');
    const track = window.kshell.verify.stream?.getVideoTracks()[0];
    return JSON.stringify({
      readyState: video.readyState,
      size: video.videoWidth + 'x' + video.videoHeight,
      paused: video.paused,
      track: track ? track.readyState : 'none',
      problem: document.getElementById('scan-problem').textContent.trim(),
    });
  })()
"""

LADDER = r"""
window.__probeLog__ = [];
window.__probeDone__ = false;
(() => {
  const log = (line) => window.__probeLog__.push(line);
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
  const video = document.getElementById('scan-video');
  const RUNGS = [
    {},
    { width: { ideal: 3840 }, height: { ideal: 2160 } },
    { width: { ideal: 1920 }, height: { ideal: 1080 } },
    { width: { ideal: 1280 }, height: { ideal: 720 } },
    { width: { ideal: 640 }, height: { ideal: 480 } },
    { height: { exact: 480 } },
    { frameRate: { ideal: 30 } },
  ];
  const named = (asked) => (JSON.stringify(asked) || '{}').replace(/"ideal":/g, '~');
  const WINDOW = 340; // the app's decode window, so the milliseconds mean what the app pays

  const countFrames = (element, track, ms) =>
    new Promise((done) => {
      let frames = 0;
      let running = true;
      const bump = () => {
        frames += 1;
        if (running && element.requestVideoFrameCallback) element.requestVideoFrameCallback(bump);
      };
      if (element.requestVideoFrameCallback) element.requestVideoFrameCallback(bump);
      else track.addEventListener('frame', () => { frames += 1; });
      setTimeout(() => { running = false; done(frames); }, ms);
    });

  const decodeCost = (element) => {
    const square = Math.min(element.videoWidth, element.videoHeight);
    if (!square || !window.jsQR) return null;
    const canvas = document.createElement('canvas');
    canvas.width = WINDOW;
    canvas.height = WINDOW;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    const started = performance.now();
    context.drawImage(
      element,
      (element.videoWidth - square) / 2,
      (element.videoHeight - square) / 2,
      square, square, 0, 0, WINDOW, WINDOW,
    );
    const pixels = context.getImageData(0, 0, WINDOW, WINDOW);
    window.jsQR(pixels.data, WINDOW, WINDOW, { inversionAttempts: 'dontInvert' });
    return Math.round(performance.now() - started);
  };

  (async () => {
    const devices = (await navigator.mediaDevices.enumerateDevices())
      .filter((device) => device.kind === 'videoinput');
    if (!devices.length) log('WebKit sees no videoinput device here');
    for (const [index, device] of devices.entries()) {
      log(`device ${index + 1}: ${device.label || '(unnamed)'}  id ${device.deviceId.slice(0, 12)}`);
      for (const rung of RUNGS) {
        const asked = { deviceId: { exact: device.deviceId }, ...rung };
        const started = performance.now();
        let stream = null;
        try {
          stream = await navigator.mediaDevices.getUserMedia({ video: asked, audio: false });
          const track = stream.getVideoTracks()[0];
          video.srcObject = stream;
          await video.play().catch(() => {});
          const frames = await countFrames(video, track, 1500);
          const settings = track.getSettings();
          let limits = null;
          try { limits = track.getCapabilities?.() ?? null; } catch (error) {}
          const decoded = decodeCost(video);
          log(
            `  ${named(rung).padEnd(40)} opened ${settings.width}x${settings.height}` +
            `@${Math.round(settings.frameRate ?? 0)} in ` +
            `${((performance.now() - started) / 1000).toFixed(1)}s` +
            ` · ${(frames / 1.5).toFixed(1)} frames/s · decode ${decoded ?? '?'}ms` +
            (limits?.width
              ? ` · publishes ${limits.width.min}-${limits.width.max}x${limits.height?.min}-${limits.height?.max}` +
                (Array.isArray(limits.frameRate) ? ` @ ${limits.frameRate.join('/')}` : '')
              : ''),
          );
        } catch (error) {
          log(
            `  ${named(rung).padEnd(40)} ${error.name}` +
            `${error.constraint ? ` (${error.constraint})` : ''} ${String(error.message).slice(0, 80)}`,
          );
        }
        for (const track of stream?.getTracks() ?? []) track.stop();
        video.srcObject = null;
        // A device released too fast is a device still claimed by the next ask.
        await sleep(500);
      }
    }
    window.__probeDone__ = true;
  })();
})();
"""


def host_cameras():
    """What the kernel and the driver say, next to what the page says: which pixel formats exist at
    which size is not visible from JavaScript, and it is the difference between a camera that refuses
    and one that was asked for something it has no format for."""
    print(f'--- /dev/video* here: {", ".join(glob.glob("/dev/video*")) or "none"}', flush=True)
    v4l2 = shutil.which('v4l2-ctl')
    for node in sorted(glob.glob('/sys/class/video4linux/*')):
        name = os.path.join(node, 'name')
        try:
            with open(name) as handle:
                print(f'    {os.path.basename(node)}: {handle.read().strip()}', flush=True)
        except OSError:
            continue
    if not v4l2:
        print('    (v4l2-ctl not installed: the pixel formats per size stay invisible)', flush=True)
        return
    for node in sorted(glob.glob('/dev/video*')):
        formats = subprocess.run(
            [v4l2, '--list-formats-ext', '-d', node], capture_output=True, text=True, check=False
        )
        print(f'--- {node} formats', flush=True)
        print(formats.stdout.strip() or formats.stderr.strip(), flush=True)


def run_ladder(view):
    printed = {'n': 0}

    def poll():
        def answer(_view, result):
            try:
                report = json.loads(view.run_javascript_finish(result).get_js_value().to_string())
            except Exception as error:  # noqa: BLE001
                print(f'ladder: {error!r}', flush=True)
                Gtk.main_quit()
                return
            for line in report['log'][printed['n']:]:
                print(line, flush=True)
            printed['n'] = len(report['log'])
            if report['done']:
                print('ladder finished', flush=True)
                Gtk.main_quit()
                return
            GLib.timeout_add(1500, poll)

        view.run_javascript(
            'JSON.stringify({ log: window.__probeLog__ ?? [], done: window.__probeDone__ === true })',
            None,
            answer,
        )
        return False

    GLib.timeout_add(1500, poll)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--offscreen', action='store_true')
    parser.add_argument('--ladder', action='store_true')
    parser.add_argument('--seconds', type=int, default=9)
    args = parser.parse_args(ARGV)

    host_cameras()

    win = Gtk.OffscreenWindow() if args.offscreen else Gtk.Window()
    view = WebKit2.WebView()
    view.set_size_request(harness.WINDOW['width'], harness.WINDOW['height'])
    win.add(view)
    win.show_all()

    def permission(_view, request):
        # A camera prompt has nobody to answer it in a probe session; the same grant the app gives.
        allowed = isinstance(request, WebKit2.PermissionMediaDevices)
        if allowed:
            request.allow()
        return allowed

    view.connect('permission-request', permission)

    ticks = {'n': 0}

    def probe(_last=None):
        ticks['n'] += 1

        def answer(_view, result):
            try:
                print(f"t+{ticks['n'] - 1}s {view.run_javascript_finish(result).get_js_value().to_string()}",
                      flush=True)
            except Exception as error:  # noqa: BLE001
                print(f"t+{ticks['n'] - 1}s {error!r}", flush=True)
            if ticks['n'] > args.seconds:
                Gtk.main_quit()
            else:
                GLib.timeout_add(1000, probe)

        view.run_javascript(PROBE, None, answer)

    def opened(_webview, event):
        if event != WebKit2.LoadEvent.FINISHED:
            return
        if args.ladder:
            view.run_javascript(LADDER, None, None)
            run_ladder(view)
            return False

        # Spaced the way a person clicks, which is what the harness learned to do the hard way.
        def drive(step=0):
            steps = ("document.getElementById('card-verify').click()",
                     "document.getElementById('btn-camera').click()")
            if step < len(steps):
                view.run_javascript(steps[step], None, None)
                GLib.timeout_add(300, drive, step + 1)
                return False
            GLib.timeout_add(1000, probe)
            return False

        drive()

    view.connect('load-changed', opened)
    view.load_html(
        harness.page('verify', camera_stub=False) if args.ladder
        else harness.page('verify-scan'),
        harness.base_uri(),
    )
    print(f'{args.offscreen and "offscreen" or "real"} window, '
          f'{harness.WINDOW["width"]}x{harness.WINDOW["height"]}, WebKitGTK stream probe', flush=True)
    GLib.timeout_add(30000 if not args.ladder else 180000, lambda: (Gtk.main_quit(), False)[1])
    Gtk.main()


if __name__ == '__main__':
    main()

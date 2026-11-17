import ctypes
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = f"{ROOT}/shots/app"
BINARY = f"{ROOT}/src-tauri/target/release/kshell-updater"

x11 = ctypes.CDLL("libX11.so.6")
xtst = ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
x11.XFlush.argtypes = [ctypes.c_void_p]
xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]

SETTLE = float(os.environ.get("SETTLE", "7"))


def shot(name):
    path = f"{OUT}/{name}.png"
    subprocess.run(["import", "-window", "root", path], check=True)
    print(f"wrote {path.replace(ROOT + '/', '')}")


def click(display, x, y):
    xtst.XTestFakeMotionEvent(display, -1, x, y, 0)
    x11.XFlush(display)
    for pressed in (1, 0):
        xtst.XTestFakeButtonEvent(display, 1, pressed, 0)
        x11.XFlush(display)
        time.sleep(0.1)


def main():
    os.makedirs(OUT, exist_ok=True)
    if not os.path.exists(BINARY):
        sys.exit(f"{BINARY} is not built; cargo build --release")
    display = x11.XOpenDisplay(None)
    if not display:
        sys.exit("no display; run me under xvfb-run")
    app = subprocess.Popen([BINARY], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(3)
        shot("01-home")
        for index, point in enumerate(sys.argv[1:], start=2):
            x, _, y = point.partition(",")
            x, y = int(x), int(y)
            click(display, x, y)
            time.sleep(SETTLE)
            shot(f"{index:02d}-click-at-{x}x{y}")
    finally:
        app.terminate()
        x11.XCloseDisplay(display)


if __name__ == "__main__":
    main()

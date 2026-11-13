# Keycard Shell Updater

Updates the firmware and the Ethereum database on a [Keycard Shell](https://shell.keycard.tech),
and verifies a device against the signed list of produced units.

Tauri 2 on Rust. The Electron implementation is parked in `legacy-electron-*` directories while
the port is in progress; it is reference material, not built or run.

## Prerequisites

- Rust (stable) — `rustup default stable`
- Node.js 20+
- Linux only: the WebKit/GTK development libraries
  (`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev`)

## Run

```sh
npm install
npm run dev      # window with hot reload
npm run build    # platform installer in src-tauri/target/release/bundle/
```

Build each platform on its own OS: macOS artefacts need macOS, Windows artefacts need Windows.

## Linux device permissions

Opening the Shell needs write access to its `/dev/hidraw` node. Install the udev rule once:

```sh
sudo cp build/linux/99-keycard-shell.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Then replug the Shell. Without it, listing works and opening fails with a permission error.

## Layout

- `src-tauri/` — Rust: HID transport, APDU framing, firmware/database loading, verification
- `frontend/` — the UI: plain HTML/CSS/JS, no bundler
- `frontend/css/app.css` — the design tokens from the agreed mockups
- `build/` — icon source and the Linux udev rule

## Device

USB vendor `0x1209`, products `0x21F7` (Keycard Shell) and `0x0001` (Beta).

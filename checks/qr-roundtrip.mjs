import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const root = `${process.cwd()}/..`;
const cargo = (args) =>
  execFileSync('cargo', ['run', '--quiet', '--manifest-path', `${root}/src-tauri/Cargo.toml`, ...args], {
    encoding: 'utf8',
  });

const output = cargo(['--example', 'units']);
const challenge = /^challenge:\s+(\S+)$/m.exec(output)?.[1];
const svg = `${root}/tests/fixtures/challenge.svg`;
if (!challenge || !fs.existsSync(svg)) {
  console.error(`the example printed no challenge, or wrote no svg:\n${output}`);
  process.exit(1);
}

const pngSide = 600;
const png = path.join(os.tmpdir(), 'kshell-challenge.png');
execFileSync('magick', ['-background', 'white', svg, '-flatten', '-resize', `${pngSide}x${pngSide}`, png]);

const decoded = cargo(['--example', 'decode', '--', png]).trim();
if (decoded !== challenge) {
  console.error(`decoded "${decoded}"\nwrote    "${challenge}"`);
  process.exit(1);
}

const rgb = path.join(os.tmpdir(), 'kshell-challenge.rgb');
execFileSync('magick', [png, '-depth', '8', rgb]);
const pixels = fs.readFileSync(rgb);
const packed = Buffer.alloc(pngSide * pngSide * 2);
for (let at = 0, out = 0; at < pixels.length; at += 6, out += 4) {
  // BT.601 in the studio swing, chroma left where grey is: a code is black and white anyway.
  for (let both = 0; both < 2; both += 1) {
    const rgbPixel = pixels.subarray(at + both * 3, at + both * 3 + 3);
    packed[out + both * 2] = Math.round(
      16 + (219 * (0.299 * rgbPixel[0] + 0.587 * rgbPixel[1] + 0.114 * rgbPixel[2])) / 255,
    );
  }
  packed[out + 1] = 128;
  packed[out + 3] = 128;
}
const raw = path.join(os.tmpdir(), 'kshell-challenge.yuyv');
fs.writeFileSync(raw, packed);
const fromCamera = cargo(['--example', 'decode', '--', '--yuyv', String(pngSide), String(pngSide), raw]).trim();
if (fromCamera !== challenge) {
  console.error(`decoded "${fromCamera}" from a YUYV frame\nwrote    "${challenge}"`);
  process.exit(1);
}

const rows = /^units list:\s+(\d+) bytes,\s+(\d+) rows,\s+build (\d+)$/m.exec(output);
console.log(
  `qr round trip ok  │ ${challenge.slice(0, 24)}… (${challenge.length} chars)`,
  `│ YUYV ok │ units list ${rows?.[2]} rows, build ${rows?.[3]}`,
);

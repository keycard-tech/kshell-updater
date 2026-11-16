import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import jsQR from '../frontend/vendor/jsQR.js';

const root = `${process.cwd()}/..`;

const output = execFileSync('cargo', ['run', '--quiet', '--manifest-path', `${root}/src-tauri/Cargo.toml`, '--example', 'units'], {
  encoding: 'utf8',
});
const challenge = /^challenge:\s+(\S+)$/m.exec(output)?.[1];
const svg = `${root}/tests/fixtures/challenge.svg`;
if (!challenge || !fs.existsSync(svg)) {
  console.error(`the example printed no challenge, or wrote no svg:\n${output}`);
  process.exit(1);
}

const size = 600;
const raw = path.join(os.tmpdir(), 'kshell-challenge.rgba');
execFileSync('magick', [
  '-background',
  'white',
  svg,
  '-flatten',
  '-resize',
  `${size}x${size}`,
  '-depth',
  '8',
  `RGBA:${raw}`,
]);

const bytes = fs.readFileSync(raw);
const found = jsQR(new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.length), size, size, {
  inversionAttempts: 'dontInvert',
});

if (!found) {
  console.error(`the shipped decoder read nothing from ${size}x${size} pixels of ${svg}`);
  process.exit(1);
}
if (found.data !== challenge) {
  console.error(`decoded "${found.data}"\nwrote    "${challenge}"`);
  process.exit(1);
}

const rows = /^units list:\s+(\d+) bytes,\s+(\d+) rows,\s+build (\d+)$/m.exec(output);
console.log(
  `qr round trip ok  │ ${challenge.slice(0, 24)}… (${challenge.length} chars)`,
  `│ units list ${rows?.[2]} rows, build ${rows?.[3]}`,
);

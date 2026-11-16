import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const wip = path.join(root, 'shots/wip');
const done = path.join(root, 'shots/done');
const frontend = path.join(root, 'frontend');

const newest = (dir) => {
  let stamp = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'dist' || entry.name.startsWith('.')) continue;
    const file = path.join(dir, entry.name);
    stamp = Math.max(stamp, entry.isDirectory() ? newest(file) : fs.statSync(file).mtimeMs);
  }
  return stamp;
};

const fresh = newest(frontend);
const wanted = process.argv.slice(2);
const shots = fs
  .readdirSync(wip)
  .filter((name) => name.endsWith('.png'))
  .filter((name) => wanted.length === 0 || wanted.includes(name.replace(/\.png$/, '')));

fs.mkdirSync(done, { recursive: true });
for (const name of shots) {
  const shot = path.join(wip, name);
  if (fs.statSync(shot).mtimeMs < fresh) {
    console.log(`${name.padEnd(30)} refused  older than frontend/`);
    continue;
  }
  fs.copyFileSync(shot, path.join(done, name));
  console.log(`${name.padEnd(30)} done`);
}

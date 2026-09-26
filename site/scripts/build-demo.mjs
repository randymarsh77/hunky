import {mkdir, copyFile, cp} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {dirname, join, resolve} from 'node:path';
import {execFileSync} from 'node:child_process';

const site = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const root = resolve(site, '..');
const output = join(site, 'static', 'playground');
await mkdir(output, {recursive: true});
execFileSync('cargo', ['build', '--locked', '--release', '--lib', '--no-default-features', '--features', 'browser', '--target', 'wasm32-unknown-unknown'], {cwd: root, stdio: 'inherit'});
execFileSync('wasm-bindgen', [
  join(root, 'target/wasm32-unknown-unknown/release/hunky.wasm'),
  '--target', 'web', '--out-dir', output, '--out-name', 'hunky',
], {cwd: root, stdio: 'inherit'});
const runtime = dirname(fileURLToPath(import.meta.resolve('@tui2web/runtime')));
await mkdir(join(output, 'runtime'), {recursive: true});
for (const file of ['index.js', 'frame.js', 'worker.js', 'style.css']) {
  await copyFile(join(runtime, file), join(output, 'runtime', file));
}
await cp(join(runtime, 'licenses'), join(output, 'runtime', 'licenses'), {recursive: true});

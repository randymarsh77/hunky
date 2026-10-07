import assert from 'node:assert/strict';
import {mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {test} from 'node:test';
import {releaseVersion} from './release-version.mjs';

const manifest = '[package]\nname = "hunky"\nversion = "0.1.0"\n\n[dependencies]\ntui2web = "=0.1.0"\n';
const lock = 'version = 4\n\n[[package]]\nname = "hunky"\nversion = "0.1.0"\n\n[[package]]\nname = "tui2web"\nversion = "0.1.0"\n';

async function fixture(t, version = '0.2.2') {
  const root = await mkdtemp(path.join(os.tmpdir(), 'hunky-release-version-'));
  t.after(() => rm(root, {recursive: true, force: true}));
  await Promise.all([
    writeFile(path.join(root, 'version.txt'), `${version}\n`),
    writeFile(path.join(root, 'Cargo.toml'), manifest),
    writeFile(path.join(root, 'Cargo.lock'), lock),
  ]);
  return root;
}

test('checks matching release, manifest and lockfile versions without writing', async t => {
  const root = await fixture(t, '0.1.0');
  assert.equal(await releaseVersion(root, '--check'), '0.1.0');
  assert.equal(await readFile(path.join(root, 'Cargo.toml'), 'utf8'), manifest);
  assert.equal(await readFile(path.join(root, 'Cargo.lock'), 'utf8'), lock);
});

test('synchronizes only the hunky package and preserves dependency versions', async t => {
  const root = await fixture(t);
  assert.equal(await releaseVersion(root, '--sync'), '0.2.2');
  assert.equal(await readFile(path.join(root, 'Cargo.toml'), 'utf8'),
    manifest.replace('version = "0.1.0"', 'version = "0.2.2"'));
  assert.equal(await readFile(path.join(root, 'Cargo.lock'), 'utf8'),
    lock.replace('version = "0.1.0"', 'version = "0.2.2"'));
  assert.equal(await releaseVersion(root, '--check'), '0.2.2');

  await writeFile(path.join(root, 'version.txt'), '0.2.3\n');
  await releaseVersion(root, '--sync');
  assert.equal(await releaseVersion(root, '--check'), '0.2.3');
});

test('rejects stale versions in either package file', async t => {
  const root = await fixture(t);
  await assert.rejects(releaseVersion(root, '--check'), /Cargo.toml has version 0.1.0/);
  await writeFile(path.join(root, 'Cargo.toml'), manifest.replace('0.1.0', '0.2.2'));
  await assert.rejects(releaseVersion(root, '--check'), /Cargo.lock has version 0.1.0/);
});

test('rejects invalid release versions', async t => {
  for (const version of ['v0.2.2', '0.2', '01.2.3', '0.2.2\nunexpected']) {
    const root = await fixture(t, version);
    await assert.rejects(releaseVersion(root, '--sync'), /Invalid release version/);
  }
});

test('validates all files before synchronizing', async t => {
  const root = await fixture(t);
  await writeFile(path.join(root, 'Cargo.lock'), 'version = 4\n');
  await assert.rejects(releaseVersion(root, '--sync'), /Expected exactly one hunky package/);
  assert.equal(await readFile(path.join(root, 'Cargo.toml'), 'utf8'), manifest);
});

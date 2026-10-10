import assert from 'node:assert/strict';
import {mkdtemp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {test} from 'node:test';
import {assembleNixCache} from './assemble-nix-cache.mjs';

const narBaseUrl = 'https://github.com/randymarsh77/hunky/releases/download/nix-cache';
const cacheInfo = 'StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 30\n';
const narinfo = [
  'StorePath: /nix/store/001gp43bjqzx60cg345n2slzg7131za8-hunky',
  'URL: nar/example.nar',
  'Compression: none',
  'NarHash: sha256:example',
  'NarSize: 1656',
  'References: ',
  'Sig: hunky-cache-1:example-signature',
  '',
].join('\n');

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'hunky-nix-cache-'));
  t.after(() => rm(root, {recursive: true, force: true}));
  const source = path.join(root, 'snapshot');
  const destination = path.join(root, 'build', 'cache');
  await mkdir(source);
  await writeFile(path.join(source, 'nix-cache-info'), cacheInfo);
  await writeFile(path.join(source, 'example.narinfo'), narinfo);
  return {source, destination};
}

test('assembles fresh or regenerated snapshots without overwriting documentation', async t => {
  const {source, destination} = await fixture(t);
  const indexPath = path.join(destination, '..', 'index.html');
  await mkdir(path.dirname(indexPath), {recursive: true});
  await writeFile(indexPath, 'Docusaurus documentation');
  await writeFile(path.join(source, '_redirects'), '/nar/:filename ignored 302\n');

  for (const baseUrl of [narBaseUrl, `${narBaseUrl}/`]) {
    await assembleNixCache(source, destination, baseUrl);
    assert.equal(await readFile(path.join(destination, 'nix-cache-info'), 'utf8'), cacheInfo);
    assert.equal(
      await readFile(path.join(destination, 'example.narinfo'), 'utf8'),
      narinfo.replace('URL: nar/example.nar', `URL: ${narBaseUrl}/example.nar`),
    );
    assert.equal(await readFile(indexPath, 'utf8'), 'Docusaurus documentation');
    await assert.rejects(readFile(path.join(destination, '_redirects')), {code: 'ENOENT'});
  }
});

test('rejects missing cache descriptors', async t => {
  const {source, destination} = await fixture(t);
  await rm(path.join(source, 'nix-cache-info'));
  await assert.rejects(assembleNixCache(source, destination, narBaseUrl), {code: 'ENOENT'});
});

test('rejects invalid cache descriptors', async t => {
  const {source, destination} = await fixture(t);
  await writeFile(path.join(source, 'nix-cache-info'), '<html>Page Not Found</html>');
  await assert.rejects(assembleNixCache(source, destination, narBaseUrl), /Invalid nix-cache-info/);
});

test('rejects empty cache snapshots', async t => {
  const {source, destination} = await fixture(t);
  await rm(path.join(source, 'example.narinfo'));
  await assert.rejects(assembleNixCache(source, destination, narBaseUrl), /no narinfo files/);
});

test('rejects missing, duplicate and unsafe NAR URLs', async t => {
  const {source, destination} = await fixture(t);
  for (const url of ['', 'URL: nar/../example.nar', 'URL: nar/example.nar\nURL: nar/other.nar']) {
    await writeFile(path.join(source, 'example.narinfo'), narinfo.replace('URL: nar/example.nar', url));
    await assert.rejects(assembleNixCache(source, destination, narBaseUrl), /Invalid NAR URL/);
  }
});

test('rejects insecure release URLs', async t => {
  const {source, destination} = await fixture(t);
  await assert.rejects(
    assembleNixCache(source, destination, narBaseUrl.replace('https:', 'http:')),
    /must use HTTPS/,
  );
});

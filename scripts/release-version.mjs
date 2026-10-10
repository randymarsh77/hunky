import {readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

function packageVersion(content, header, filename) {
  const blocks = content.split(/(?=^\[)/m).filter(block =>
    block.split(/\r?\n/, 1)[0] === header && /^name = "hunky"\r?$/m.test(block));
  if (blocks.length !== 1) {
    throw new Error(`Expected exactly one hunky package in ${filename}`);
  }
  const block = blocks[0];
  const versions = [...block.matchAll(/^version = "([^"]+)"\r?$/gm)];
  if (versions.length !== 1) {
    throw new Error(`Expected exactly one package version in ${filename}`);
  }
  return {block, version: versions[0][1]};
}

export async function releaseVersion(root, mode) {
  if (mode !== '--check' && mode !== '--sync') {
    throw new Error('Usage: release-version.mjs --check|--sync');
  }
  const version = (await readFile(path.join(root, 'version.txt'), 'utf8')).trim();
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error(`Invalid release version: ${version}`);
  }

  const files = await Promise.all([
    ['Cargo.toml', '[package]'],
    ['Cargo.lock', '[[package]]'],
  ].map(async ([filename, header]) => {
    const file = path.join(root, filename);
    const content = await readFile(file, 'utf8');
    const metadata = packageVersion(content, header, filename);
    return {file, filename, content, ...metadata};
  }));

  if (mode === '--check') {
    for (const file of files) {
      if (file.version !== version) {
        throw new Error(`${file.filename} has version ${file.version}; expected ${version} from version.txt`);
      }
    }
  } else {
    await Promise.all(files.map(file => writeFile(file.file,
      file.content.replace(file.block,
        file.block.replace(/^version = "[^"]+"/m, `version = "${version}"`)))));
  }
  return version;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await releaseVersion(process.cwd(), process.argv[2]);
}

import {mkdir, readdir, readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

export async function assembleNixCache(sourceDir, destinationDir, narBaseUrl) {
  const baseUrl = new URL(narBaseUrl);
  if (baseUrl.protocol !== 'https:') {
    throw new Error('NAR release URL must use HTTPS');
  }

  const cacheInfo = await readFile(path.join(sourceDir, 'nix-cache-info'), 'utf8');
  if (!/^StoreDir: \/nix\/store$/m.test(cacheInfo)) {
    throw new Error('Invalid nix-cache-info: expected StoreDir: /nix/store');
  }

  const files = (await readdir(sourceDir)).filter(file => file.endsWith('.narinfo'));
  if (files.length === 0) {
    throw new Error('Binary cache contains no narinfo files');
  }

  const narinfos = await Promise.all(files.map(async file => {
    const content = await readFile(path.join(sourceDir, file), 'utf8');
    const urls = [...content.matchAll(/^URL: (.+)$/gm)];
    if (urls.length !== 1 || !/^nar\/[A-Za-z0-9._-]+$/.test(urls[0][1])) {
      throw new Error(`Invalid NAR URL in ${file}`);
    }

    // GitHub Pages ignores the generator's Cloudflare _redirects file.
    // Nix signatures cover store metadata, not this download URL.
    const url = `${baseUrl.href.replace(/\/$/, '')}/${urls[0][1].slice(4)}`;
    return [file, content.replace(/^URL: .+$/m, `URL: ${url}`)];
  }));

  await mkdir(destinationDir, {recursive: true});
  await writeFile(path.join(destinationDir, 'nix-cache-info'), cacheInfo);
  await Promise.all(narinfos.map(([file, content]) =>
    writeFile(path.join(destinationDir, file), content)));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [sourceDir, destinationDir, narBaseUrl] = process.argv.slice(2);
  if (!sourceDir || !destinationDir || !narBaseUrl) {
    throw new Error('Usage: assemble-nix-cache.mjs SOURCE DESTINATION NAR_RELEASE_URL');
  }
  await assembleNixCache(sourceDir, destinationDir, narBaseUrl);
}

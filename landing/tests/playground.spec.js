import {test, expect} from '@playwright/test';

const shell = page => page.locator('#playground');
const screen = page => page.frameLocator('iframe').locator('.xterm-rows');
const input = page => page.frameLocator('iframe').locator('.xterm-helper-textarea');
async function ready(page) {
  await page.goto('/');
  await page.getByRole('button', {name: 'Try it live'}).click();
  await expect(shell(page)).toHaveAttribute('data-state', 'running');
  await expect(input(page)).toBeFocused();
}
async function geometry(page) {
  return page.evaluate(() => ['#playground', '#features'].map(selector => {
    const box = document.querySelector(selector).getBoundingClientRect();
    return {width: box.width, height: box.height, top: box.top + scrollY};
  }));
}

test('cold landing downloads only the image and small shell; click alone starts download and shimmer', async ({page}) => {
  const requests = [];
  page.on('request', request => requests.push(new URL(request.url()).pathname));
  let finishDownload;
  const download = new Promise(resolve => { finishDownload = resolve; });
  await page.route('**/playground/hunky_bg.wasm', async route => { await download; await route.continue(); });
  await page.goto('/');
  await page.waitForLoadState('networkidle');
  await expect(page.locator('#demo-image')).toBeVisible();
  expect(await page.locator('#demo-image').evaluate(img => img.complete && img.naturalWidth === 1109)).toBe(true);
  expect(requests).toContain('/terminal.png');
  expect(requests.filter(path => path.startsWith('/playground/'))).toEqual([]);
  expect(page.workers()).toHaveLength(0);
  await expect(page.locator('iframe, link[rel="preload"], link[rel="prefetch"], link[rel="modulepreload"]')).toHaveCount(0);
  await expect(shell(page).locator('details, textarea, input, pre, [data-key], [data-demo-action]')).toHaveCount(0);
  const shellBytes = await page.evaluate(() => performance.getEntriesByType('resource')
    .filter(entry => /\/playground\.(js|css)$/.test(entry.name))
    .reduce((sum, entry) => sum + entry.decodedBodySize, 0));
  expect(shellBytes).toBeGreaterThan(0);
  expect(shellBytes).toBeLessThan(10000);
  const before = await geometry(page);
  await page.getByRole('button', {name: 'Try it live'}).click();
  await expect(shell(page)).toHaveAttribute('data-state', 'loading');
  await expect(page.getByRole('status')).toHaveText('Loading demo…');
  await expect(page.locator('#demo-overlay')).toBeFocused();
  await expect(page.locator('#demo-image')).toBeHidden();
  expect(await page.locator('#demo-overlay').evaluate(e => getComputedStyle(e).animationName)).toBe('demo-shimmer');
  expect(await geometry(page)).toEqual(before);
  finishDownload();
  await expect(shell(page)).toHaveAttribute('data-state', 'running');
  expect(await geometry(page)).toEqual(before);
  expect(requests).toEqual(expect.arrayContaining(['/playground/runtime/index.js', '/playground/runtime/frame.js',
    '/playground/runtime/worker.js', '/playground/hunky.js', '/playground/hunky_bg.wasm']));
  await expect(input(page)).toBeFocused();
});

test('actual keyboard staging, quit and keyboard Restart create a fresh demo without navigation', async ({page}) => {
  await ready(page);
  const url = page.url();
  await expect(screen(page)).not.toContainText('[STAGED');
  await page.keyboard.press('s');
  await expect(screen(page)).toContainText('[STAGED');
  await page.keyboard.press('s');
  await expect(screen(page)).not.toContainText('[STAGED');
  await page.keyboard.press('s');
  await expect(screen(page)).toContainText('[STAGED');
  await page.keyboard.press('q');
  await expect(page.getByRole('status')).toHaveText('Hunky exited. Restart to try again.');
  await expect(page.getByRole('link', {name: 'Restart'})).toBeFocused();
  await expect(page.locator('iframe')).toHaveCount(0);
  await expect.poll(() => page.workers().length).toBe(0);
  await page.keyboard.press('Enter');
  await expect(shell(page)).toHaveAttribute('data-state', 'running');
  await expect(input(page)).toBeFocused();
  await expect(screen(page)).not.toContainText('[STAGED');
  await page.keyboard.press('s');
  await expect(screen(page)).toContainText('[STAGED');
  expect(page.url()).toBe(url);
  await expect(page.locator('iframe')).toHaveCount(1);
});

for (const asset of ['runtime/index.js', 'hunky_bg.wasm']) {
  test(`failed ${asset} download provides a working Retry link`, async ({page}) => {
    const path = `**/playground/${asset}*`;
    await page.route(path, route => route.abort());
    await page.goto('/');
    await page.getByRole('button', {name: 'Try it live'}).click();
    await expect(page.getByRole('status')).toHaveText('Demo unavailable. Retry.');
    await expect(page.getByRole('link', {name: 'Retry'})).toBeFocused();
    await page.unroute(path);
    await page.keyboard.press('Enter');
    await expect(shell(page)).toHaveAttribute('data-state', 'running');
    await expect(input(page)).toBeFocused();
    await expect(page.locator('iframe')).toHaveCount(1);
  });
}

for (const width of [1280, 390]) {
  test(`original screenshot proportions and single chrome stay stable at ${width}px`, async ({page}, testInfo) => {
    await page.setViewportSize({width, height: 900});
    await page.goto('/');
    const before = await geometry(page);
    expect(before[0].width / before[0].height).toBeCloseTo(1109 / 679, 3);
    await expect(page.locator('#demo-window')).toBeHidden();
    await shell(page).screenshot({path: testInfo.outputPath('screenshot-first.png')});
    const button = await page.locator('#demo-start').boundingBox();
    const image = await page.locator('#demo-image').boundingBox();
    expect(button.x + button.width / 2).toBeCloseTo(image.x + image.width / 2, 0);
    expect(button.y + button.height / 2).toBeCloseTo(image.y + image.height / 2, 0);
    await page.getByRole('button', {name: 'Try it live'}).click();
    await expect(shell(page)).toHaveAttribute('data-state', 'running');
    await expect(page.locator('#demo-image')).toBeHidden();
    await expect(page.locator('.window-titlebar:visible')).toHaveCount(1);
    await expect(page.locator('.window-titlebar')).toHaveAttribute('aria-hidden', 'true');
    expect(await geometry(page)).toEqual(before);
    await page.locator('iframe').scrollIntoViewIfNeeded();
    await expect.poll(async () => (await page.frameLocator('iframe').locator('.xterm-screen').boundingBox()).width).toBeLessThan(before[0].width);
    await expect(screen(page)).toContainText('Hello from Orchard!');
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await shell(page).screenshot({path: testInfo.outputPath('live-terminal.png')});
  });
}

test('reduced motion, repeated activation and page lifecycle do not duplicate or eagerly resume runtimes', async ({page}) => {
  await page.emulateMedia({reducedMotion: 'reduce'});
  let finishDownload;
  const download = new Promise(resolve => { finishDownload = resolve; });
  await page.route('**/playground/hunky_bg.wasm', async route => { await download; await route.continue(); });
  await page.goto('/');
  await page.locator('#demo-start').evaluate(button => { button.click(); button.click(); });
  await expect(shell(page)).toHaveAttribute('data-state', 'loading');
  expect(await page.locator('#demo-overlay').evaluate(e => getComputedStyle(e).animationName)).toBe('none');
  await page.evaluate(() => dispatchEvent(new PageTransitionEvent('pagehide', {persisted: true})));
  finishDownload();
  await expect.poll(() => page.workers().length).toBe(0);
  await expect(page.locator('iframe')).toHaveCount(0);
  await page.evaluate(() => dispatchEvent(new PageTransitionEvent('pageshow', {persisted: true})));
  await expect(shell(page)).toHaveAttribute('data-state', 'idle');
  await page.locator('#demo-start').evaluate(button => { button.click(); button.click(); });
  await expect(shell(page)).toHaveAttribute('data-state', 'running');
  await expect(page.locator('iframe')).toHaveCount(1);
  await page.evaluate(() => dispatchEvent(new PageTransitionEvent('pagehide', {persisted: true})));
  await expect(page.locator('iframe')).toHaveCount(0);
  await expect.poll(() => page.workers().length).toBe(0);
});

test('test-only harness preserves line/hunk semantics, edits, commits, complete reset and instance isolation', async ({page}) => {
  await page.goto('/');
  const result = await page.evaluate(async () => {
    const {mountIsolated} = await import('/playground/runtime/index.js');
    const containers = [document.createElement('div'), document.createElement('div')];
    containers.forEach(container => {
      container.style.cssText = 'width:1000px;height:500px';
      document.body.append(container);
    });
    const options = {
      moduleUrl: new URL('/playground/hunky.js', location.href),
      runtimeBaseUrl: new URL('/playground/runtime/', location.href),
    };
    const first = await mountIsolated(containers[0], options);
    const second = await mountIsolated(containers[1], options);
    const state = async handle => {
      const snapshot = await handle.snapshot();
      return JSON.parse(new TextDecoder().decode(new Uint8Array(snapshot.files.find(([path]) => path === 'demo.json')[1])));
    };
    const key = text => first.send({type: 'text', text});
    const command = value => first.send({type: 'paste', text: JSON.stringify(value)});
    try {
      const initial = await state(first);
      await key('l'); await key('s');
      const line = await state(first);
      await key('s');
      const unstaged = await state(first);
      await key('l'); await key('s');
      const hunk = await state(first);
      await key(' '); await key('s');
      const both = await state(first);
      const content = initial.files.find(file => file.path === 'src/shop.rs').content;
      await command({action: 'edit', path: 'src/shop.rs', content: content.replace('Hello from Orchard!', 'Welcome to Orchard')});
      const edited = await state(first);
      const other = await state(second);
      await first.reset();
      const reset = await state(first);
      await key('n'); await key('n'); await key('s'); await key('n'); await key('s');
      const files = await state(first);
      await command({action: 'commit', message: 'Test simulated commit'});
      const committed = await state(first);
      await command({action: 'commit', message: 'Nothing staged'});
      const error = await state(first);
      return {initial, line, unstaged, hunk, both, edited, other, reset, files, committed, error};
    } finally {
      await Promise.all([first.dispose(), second.dispose()]);
      containers.forEach(container => container.remove());
    }
  });
  expect(result.line.stagedDiff).toContain('+    "Hello from Orchard!"');
  expect(result.line.stagedDiff).not.toContain('-    "Welcome to Orchard"');
  expect(result.unstaged.stagedDiff).not.toContain('src/shop.rs');
  expect(result.hunk.stagedDiff).toContain('-    "Welcome to Orchard"');
  expect(result.hunk.stagedDiff).not.toContain('Apples and cinnamon');
  expect(result.both.stagedDiff).toContain('Apples and cinnamon');
  expect(result.edited.repository.index).toEqual(result.both.repository.index);
  expect(result.edited.unstagedDiff).toContain('+    "Welcome to Orchard"');
  expect(result.other.repository).toEqual(result.initial.repository);
  expect(result.reset.repository).toEqual(result.initial.repository);
  expect(result.files.stagedDiff).toContain('+Try staging just one line.');
  expect(result.files.stagedDiff).toContain('-Remove this old inventory export.');
  expect(result.committed.message).toContain('Created simulated commit');
  expect(result.committed.stagedDiff).toBe('');
  expect(result.error.message).toContain('Error:');
  await expect(page.locator('iframe')).toHaveCount(0);
});

test('public demo retains opaque origin and denies guest network and storage', async ({page}) => {
  await ready(page);
  await expect(page.locator('iframe')).toHaveAttribute('sandbox', 'allow-scripts');
  expect(await page.locator('iframe').evaluate(frame => frame.contentDocument === null)).toBe(true);
  const frame = page.frames().find(frame => frame.url() === 'about:srcdoc');
  const denied = await frame.evaluate(async () => {
    let storage = false;
    let network = false;
    try { localStorage.setItem('probe', 'blocked'); } catch { storage = true; }
    try { await fetch('https://example.com/'); } catch { network = true; }
    return {storage, network, origin: self.origin};
  });
  expect(denied).toEqual({storage: true, network: true, origin: 'null'});
});

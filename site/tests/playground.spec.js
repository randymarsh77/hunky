import {test, expect} from '@playwright/test';

async function ready(page) {
  await page.goto('/hunky/');
  await expect(page.getByTestId('runtime-status')).toContainText('Ready.');
  await expect(page.getByLabel('File contents')).toContainText('Hello from Orchard!');
}

test('real terminal stages and reverses a line, a hunk, and the next hunk', async ({page}) => {
  await ready(page);
  const staged = page.getByTestId('staged-diff');
  await expect(staged).not.toContainText('Hello from Orchard!');
  await page.getByRole('button', {name: 'Line mode', exact: true}).click();
  await page.getByRole('button', {name: 'Stage / unstage selection'}).click();
  await expect(staged).toContainText('+    "Hello from Orchard!"');
  await expect(staged).not.toContainText('-    "Welcome to Orchard"');
  await page.getByRole('button', {name: 'Stage / unstage selection'}).click();
  await expect(staged).not.toContainText('src/shop.rs');
  await page.getByRole('button', {name: 'Line mode', exact: true}).click();
  await page.getByRole('button', {name: 'Stage / unstage selection'}).click();
  await expect(staged).toContainText('+    "Hello from Orchard!"');
  await expect(staged).not.toContainText('Apples and cinnamon');
  await page.getByRole('button', {name: 'Next hunk', exact: true}).click();
  await page.getByRole('button', {name: 'Stage / unstage selection'}).click();
  await expect(staged).toContainText('Apples and cinnamon');
  await page.getByRole('button', {name: 'Restart / reset fixture'}).click();
  await expect(staged).not.toContainText('Hello from Orchard!');
  await expect(staged).toContainText('Now with seasonal specials.');
});

test('keyboard staging, edits, index-only reversal, and isolated instances', async ({page, context}) => {
  await ready(page);
  const second = await context.newPage();
  await ready(second);
  const terminal = page.frameLocator('iframe[title="Isolated tui2web playground"]');
  await terminal.locator('.xterm-helper-textarea').focus();
  await page.keyboard.press('s');
  await page.getByRole('button', {name: 'Refresh inspector'}).click();
  await expect(page.getByTestId('staged-diff')).toContainText('Hello from Orchard!');
  const current = await page.getByLabel('File contents').inputValue();
  await page.getByLabel('File contents').fill(current.replace('Hello from Orchard!', 'Welcome to Orchard'));
  await page.getByRole('button', {name: 'Save working-tree edit'}).click();
  await expect(page.getByTestId('staged-diff')).toContainText('Hello from Orchard!');
  await expect(page.getByTestId('unstaged-diff')).toContainText('+    "Welcome to Orchard"');
  await second.getByRole('button', {name: 'Refresh inspector'}).click();
  await expect(second.getByTestId('staged-diff')).not.toContainText('Hello from Orchard!');
  await expect(second.getByLabel('File contents')).toHaveValue(current);
  await expect(page.locator('iframe')).toHaveAttribute('sandbox', 'allow-scripts');
  expect(await page.locator('iframe').evaluate(frame => frame.contentDocument === null)).toBe(true);
  await page.bringToFront();
  await page.setViewportSize({width: 390, height: 844});
  await page.locator('iframe').scrollIntoViewIfNeeded();
  await expect(page.getByTestId('runtime-status')).toContainText('Ready.');
  const screen = page.frameLocator('iframe').locator('.xterm-screen');
  await expect.poll(async () => (await screen.boundingBox()).width).toBeLessThan(390);
  await page.getByRole('button', {name: 'Restart / reset fixture'}).click();
  await expect(page.getByLabel('File contents')).toHaveValue(current);
});

test('same-page instances reset complete state and deny origin storage and network', async ({page}) => {
  await ready(page);
  const result = await page.evaluate(async () => {
    const {mountIsolated} = await import('/hunky/playground/runtime/index.js');
    const containers = [document.createElement('div'), document.createElement('div')];
    containers.forEach(container => {
      container.style.cssText = 'width:800px;height:400px';
      document.body.append(container);
    });
    const options = {
      moduleUrl: new URL('/hunky/playground/hunky.js', location.href),
      runtimeBaseUrl: new URL('/hunky/playground/runtime/', location.href),
    };
    const first = await mountIsolated(containers[0], options);
    const second = await mountIsolated(containers[1], options);
    const state = async handle => {
      const snapshot = await handle.snapshot();
      return JSON.parse(new TextDecoder().decode(new Uint8Array(snapshot.files.find(([path]) => path === 'demo.json')[1])));
    };
    const initial = await state(first);
    await first.send({type:'text', text:'s'});
    const changed = await state(first);
    const other = await state(second);
    await first.reset();
    const reset = await state(first);
    await Promise.all([first.dispose(), second.dispose()]);
    const cleaned = containers.every(container => container.children.length === 0);
    containers.forEach(container => container.remove());
    return {initial: initial.repository, changed: changed.repository, other: other.repository, reset: reset.repository, cleaned};
  });
  expect(result.changed.index).not.toEqual(result.initial.index);
  expect(result.changed.head).toEqual(result.initial.head);
  expect(result.other).toEqual(result.initial);
  expect(result.reset).toEqual(result.initial);
  expect(result.cleaned).toBe(true);
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

test('untracked additions, deletion staging, history, and commit errors', async ({page}) => {
  await ready(page);
  // File navigation from shop wraps to README, notes, then obsolete.
  await page.getByRole('button', {name: 'Next file', exact: true}).click();
  await page.getByRole('button', {name: 'Next file', exact: true}).click();
  await page.getByRole('button', {name: 'Stage / unstage selection'}).click();
  await expect(page.getByTestId('staged-diff')).toContainText('+Try staging just one line.');
  await page.getByRole('button', {name: 'Next file', exact: true}).click();
  await page.getByRole('button', {name: 'Stage / unstage selection'}).click();
  await expect(page.getByTestId('staged-diff')).toContainText('-Remove this old inventory export.');
  await page.getByRole('button', {name: 'Commit staged changes (simulated)'}).click();
  await expect(page.getByTestId('demo-message')).toContainText('Created simulated commit');
  await expect(page.getByTestId('staged-diff')).toHaveText('No staged changes');
  await page.getByRole('button', {name: 'Commit staged changes (simulated)'}).click();
  await expect(page.getByTestId('demo-message')).toContainText('Error:');
  await page.getByRole('button', {name: 'Review history'}).click();
  const terminal = page.frameLocator('iframe').locator('.xterm-helper-textarea');
  await terminal.focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('runtime-status')).toContainText('Ready.');
});

test('loading failure is visible and retry recovers', async ({page}) => {
  await page.route('**/playground/hunky_bg.wasm', route => route.abort());
  await page.goto('/hunky/');
  await expect(page.getByTestId('runtime-status')).toContainText('Demo unavailable');
  await expect(page.getByRole('button', {name: 'Stage / unstage selection'})).toBeDisabled();
  await page.unroute('**/playground/hunky_bg.wasm');
  await page.getByRole('button', {name: 'Restart / reset fixture'}).click();
  await expect(page.getByTestId('runtime-status')).toContainText('Ready.');
});

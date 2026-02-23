/**
 * main.js – hunky web demo frontend
 *
 * Loads the WASM module produced by `wasm-pack`, creates an xterm.js terminal,
 * and runs the hunky demo application inside it.
 */

import init, { App } from './pkg/hunky_demo.js';

const COLS = 100;
const ROWS = 30;

async function run() {
  const statusEl = document.getElementById('status');

  try {
    await init();
  } catch (err) {
    statusEl.textContent = `Failed to load WebAssembly module: ${err.message}`;
    console.error(err);
    return;
  }

  const term = new Terminal({
    cols: COLS,
    rows: ROWS,
    fontFamily: '"Cascadia Code", "Fira Code", "JetBrains Mono", "Courier New", monospace',
    fontSize: 14,
    lineHeight: 1.1,
    theme: {
      background: '#1e1e2e',
      foreground: '#cdd6f4',
      cursor:     '#f5e0dc',
      selectionBackground: '#45475a',
    },
    cursorBlink: true,
    allowProposedApi: true,
  });

  const fitAddon = new FitAddon.FitAddon();
  term.loadAddon(fitAddon);

  const wrapper = document.getElementById('terminal-wrapper');
  term.open(wrapper);
  fitAddon.fit();

  const app = new App(term.cols, term.rows);

  term.onKey(({ domEvent }) => {
    if (!app.should_quit()) {
      app.push_key(domEvent.key);
      app.tick();
      term.write(app.get_frame());
    }
    domEvent.preventDefault();
  });

  app.tick();
  term.write(app.get_frame());

  statusEl.textContent = 'Click the terminal and press H for help. Use j/k to navigate hunks, J/K for files.';

  function renderLoop() {
    if (app.should_quit()) {
      term.write(
        '\r\n\x1b[32mDemo has quit.\x1b[0m Refresh the page to restart.\r\n',
      );
      statusEl.textContent = 'Demo has quit. Refresh to restart.';
      return;
    }

    const running = app.tick();
    term.write(app.get_frame());

    if (running) {
      requestAnimationFrame(renderLoop);
    }
  }

  requestAnimationFrame(renderLoop);

  const resizeObserver = new ResizeObserver(() => {
    fitAddon.fit();
    app.resize(term.cols, term.rows);
    app.tick();
    term.write(app.get_frame());
  });
  resizeObserver.observe(wrapper);

  term.focus();
}

run().catch(console.error);

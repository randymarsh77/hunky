import React, {useEffect, useRef, useState} from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

const keyInput = key => ({
  type: 'key', key, code: '', repeat: false,
  modifiers: {ctrl: false, alt: false, shift: false, meta: false},
});

export default function Playground() {
  const container = useRef(null);
  const handle = useRef(null);
  const assets = useBaseUrl('/playground/');
  const [status, setStatus] = useState({state: 'starting'});
  const [view, setView] = useState(null);
  const [path, setPath] = useState('src/shop.rs');
  const [content, setContent] = useState('');
  const [message, setMessage] = useState('Demo commit');
  const [busy, setBusy] = useState(false);
  const [generation, setGeneration] = useState(0);
  const active = status.state === 'running' && !busy;

  async function inspect(runtime = handle.current, loadEditor = false) {
    const snapshot = await runtime.snapshot();
    const file = snapshot.files.find(([path]) => path === 'demo.json');
    if (!file) throw new Error('The demo did not provide repository state');
    const state = JSON.parse(new TextDecoder().decode(new Uint8Array(file[1])));
    setView(state);
    if (loadEditor) setContent(state.files.find(file => file.path === path)?.content ?? '');
    return state;
  }

  useEffect(() => {
    let cancelled = false;
    let runtime;
    setStatus({state: 'starting'});
    setView(null);
    (async () => {
      // Load only in the browser: neither xterm nor WASM participates in SSR.
      const {mountIsolated} = await import(/* webpackIgnore: true */ new URL(`${assets}runtime/index.js`, window.location.href).href);
      if (cancelled) return;
      runtime = await mountIsolated(container.current, {
        moduleUrl: new URL(`${assets}hunky.js`, window.location.href),
        wasmUrl: new URL(`${assets}hunky_bg.wasm`, window.location.href),
        runtimeBaseUrl: new URL(`${assets}runtime/`, window.location.href),
        columns: 100, rows: 30,
        onStatus: next => { if (!cancelled) setStatus(next); },
      });
      if (cancelled) { await runtime.dispose(); return; }
      handle.current = runtime;
      await inspect(runtime, true);
    })().catch(error => {
      if (!cancelled) setStatus({state: 'error', error: error.message});
    });
    return () => {
      cancelled = true;
      handle.current = null;
      if (runtime) void runtime.dispose().catch(error => console.error('Playground cleanup failed', error));
    };
  }, [assets, generation]);

  async function perform(operation) {
    setBusy(true);
    try {
      await operation(handle.current);
      await inspect();
    } catch (error) {
      setStatus({state: 'error', error: error.message});
    } finally {
      setBusy(false);
    }
  }

  const sendKey = key => perform(async runtime => {
    await runtime.send(keyInput(key));
    runtime.focus();
  });
  const command = value => perform(runtime => runtime.send({type: 'paste', text: JSON.stringify(value)}));

  return (
    <section className={`container ${styles.playground}`} aria-label="Hunky browser playground">
      <h2>Try the real Hunky UI</h2>
      <p>
        This is Hunky compiled to WebAssembly, using a private <strong>browser-local simulated Git repository</strong>.
        No sign-in, server, or access to your repositories. Git history and staging are simulated;
        remotes, shell commands, and external editors are unavailable. Nothing is saved after a restart.
      </p>
      <div className={styles.toolbar}>
        <button disabled={!active} onClick={() => sendKey('s')}>Stage / unstage selection</button>
        <button disabled={!active} onClick={() => sendKey('l')}>Line mode</button>
        <button disabled={!active} onClick={() => sendKey('j')}>Next line</button>
        <button disabled={!active} onClick={() => sendKey(' ')}>Next hunk</button>
        <button disabled={!active} onClick={() => sendKey('n')}>Next file</button>
        <button disabled={!active} onClick={() => sendKey('r')}>Review history</button>
        <button disabled={!active} onClick={() => sendKey('H')}>Help</button>
        <button disabled={busy} onClick={() => {
          setPath('src/shop.rs');
          setGeneration(value => value + 1);
        }}>Restart / reset fixture</button>
      </div>
      <p role="status" data-testid="runtime-status">
        {status.state === 'starting' ? 'Loading the isolated demo...' :
          status.state === 'running' ? 'Ready. Click the terminal to use the keyboard.' :
          status.state === 'exited' ? 'Hunky exited. Restart to try again.' :
          `Demo unavailable: ${status.error ?? status.state}. Restart to retry.`}
      </p>
      <div ref={container} className={styles.terminal} data-testid="terminal" />
      <p className={styles.hint}>
        <kbd>S</kbd> toggles the selected hunk or line. <kbd>L</kbd> switches line mode;
        <kbd>J</kbd>/<kbd>K</kbd> select lines. <kbd>Space</kbd>/<kbd>B</kbd> move between hunks,
        <kbd>N</kbd>/<kbd>P</kbd> between files. <kbd>Tab</kbd> focuses the file list for whole-file staging.
        <kbd>Esc</kbd> leaves review/help. Index-only hunks preserve staged edits even if you undo them in the working tree.
      </p>
      <details open className={styles.inspector}>
        <summary>Working-tree editor and repository inspector</summary>
        <p>Edit a file, then stage a line or hunk in the terminal. Editing never changes the index.</p>
        <label>File path
          <input aria-label="File path" value={path} onChange={event => setPath(event.target.value)} list={`files-${generation}`} />
        </label>
        <datalist id={`files-${generation}`}>
          {view?.files.map(file => <option key={file.path} value={file.path} />)}
        </datalist>
        <button disabled={!active} onClick={() => perform(async runtime => { await inspect(runtime, true); })}>Load file</button>
        <label>File contents
          <textarea aria-label="File contents" spellCheck={false} rows={9} value={content} onChange={event => setContent(event.target.value)} />
        </label>
        <div className={styles.toolbar}>
          <button disabled={!active} onClick={() => command({action: 'edit', path, content})}>Save working-tree edit</button>
          <button disabled={!active} onClick={() => command({action: 'delete', path})}>Delete working-tree file</button>
          <button disabled={!active} onClick={() => perform(async () => {})}>Refresh inspector</button>
        </div>
        <p role="status" data-testid="demo-message">{view?.message}</p>
        <label>Commit message
          <input aria-label="Commit message" value={message} onChange={event => setMessage(event.target.value)} />
        </label>
        <button disabled={!active} onClick={() => command({action: 'commit', message})}>Commit staged changes (simulated)</button>
        <div className={styles.diffs}>
          <div><h3>Staged: HEAD → index</h3><pre data-testid="staged-diff">{view?.stagedDiff || 'No staged changes'}</pre></div>
          <div><h3>Unstaged: index → working tree</h3><pre data-testid="unstaged-diff">{view?.unstagedDiff || 'No unstaged changes'}</pre></div>
        </div>
      </details>
    </section>
  );
}

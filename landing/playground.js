const shell = document.getElementById('playground');
const image = document.getElementById('demo-image');
const start = document.getElementById('demo-start');
const windowElement = document.getElementById('demo-window');
const terminal = document.getElementById('terminal');
const overlay = document.getElementById('demo-overlay');
const message = document.getElementById('demo-message');
let runtime;
let mountIsolated;
let generation = 0;
let importAttempts = 0;
let cleanup = Promise.resolve();

function show(state) {
	shell.dataset.state = state;
	image.hidden = state !== 'idle';
	start.hidden = state !== 'idle';
	windowElement.hidden = state === 'idle';
	overlay.hidden = state === 'running' || state === 'idle';
	terminal.inert = state !== 'running';
	terminal.setAttribute('aria-busy', String(state === 'loading'));
	message.replaceChildren();
	if (state === 'loading') {
		message.textContent = 'Loading demo…';
		overlay.focus({preventScroll: true});
	} else if (state === 'exited' || state === 'error') {
		const retry = document.createElement('a');
		retry.href = '#playground';
		retry.textContent = state === 'exited' ? 'Restart' : 'Retry';
		retry.addEventListener('click', activate);
		message.append(state === 'exited' ? 'Hunky exited. ' : 'Demo unavailable. ', retry,
			state === 'exited' ? ' to try again.' : '.');
		retry.focus({preventScroll: true});
	}
}

function release() {
	const previous = runtime;
	runtime = undefined;
	if (previous) cleanup = previous.dispose().catch(error => {
		console.error('Hunky demo cleanup failed', error);
	});
	return cleanup;
}

async function activate(event) {
	event.preventDefault();
	if (shell.dataset.state === 'loading' || shell.dataset.state === 'running') return;
	const current = ++generation;
	show('loading');
	try {
		await release();
		if (!mountIsolated) {
			const url = new URL('./playground/runtime/index.js', import.meta.url);
			if (importAttempts++) url.searchParams.set('retry', String(importAttempts));
			({mountIsolated} = await import(url.href));
		}
		if (current !== generation) return;
		// A stale in-flight mount must never append into a newer attempt's container.
		const container = document.createElement('div');
		container.className = 'demo-runtime';
		terminal.replaceChildren(container);
		const handle = await mountIsolated(container, {
			moduleUrl: new URL('./playground/hunky.js', import.meta.url),
			wasmUrl: new URL('./playground/hunky_bg.wasm', import.meta.url),
			runtimeBaseUrl: new URL('./playground/runtime/', import.meta.url),
			columns: 100, rows: 30,
			onStatus: next => {
				if (current !== generation) return;
				if (next.state === 'running') show('running');
				if (next.state === 'exited' || next.state === 'error') {
					if (next.error) console.error('Hunky demo failed', next.error);
					show(next.state);
					void release();
				}
			},
		});
		if (current !== generation) { await handle.dispose(); return; }
		runtime = handle;
		if (handle.status.state === 'running') handle.focus();
		else await release();
	} catch (error) {
		if (current === generation) {
			console.error('Hunky demo failed', error);
			show('error');
		}
	}
}

start.addEventListener('click', activate);
window.addEventListener('pagehide', () => {
	generation++;
	void release();
});
window.addEventListener('pageshow', event => {
	if (event.persisted) show('idle');
});

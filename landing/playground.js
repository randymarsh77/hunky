const element = id => document.getElementById(id);
const controls = document.querySelectorAll('[data-key], [data-demo-action]');
let runtime;
let generation = 0;
let busy = false;
let status = {state: 'starting'};

function renderStatus(next = status) {
	status = next;
	controls.forEach(control => { control.disabled = busy || status.state !== 'running'; });
	element('demo-reset').disabled = busy;
	element('runtime-status').textContent = status.state === 'running'
		? 'Ready. Click the terminal; S stages, L selects lines.'
		: status.state === 'starting' ? 'Loading the isolated demo…'
		: status.state === 'exited' ? 'Hunky exited. Restart to try again.'
		: `Demo unavailable: ${status.error ?? status.state}. Restart to retry.`;
	element('demo-overlay').hidden = status.state === 'running';
	element('load-message').textContent = element('runtime-status').textContent;
}

async function inspect(handle = runtime, loadEditor = false) {
	const snapshot = await handle.snapshot();
	const file = snapshot.files.find(([path]) => path === 'demo.json');
	if (!file) throw new Error('The demo did not provide repository state');
	const view = JSON.parse(new TextDecoder().decode(new Uint8Array(file[1])));
	element('demo-message').textContent = view.message;
	element('staged-diff').textContent = view.stagedDiff || 'No staged changes';
	element('unstaged-diff').textContent = view.unstagedDiff || 'No unstaged changes';
	element('demo-files').replaceChildren(...view.files.map(file => {
		const option = document.createElement('option');
		option.value = file.path;
		return option;
	}));
	if (loadEditor) element('demo-content').value = view.files.find(file => file.path === element('demo-path').value)?.content ?? '';
}

async function mount() {
	const current = ++generation;
	const previous = runtime;
	runtime = undefined;
	element('demo-path').value = 'src/shop.rs';
	renderStatus({state: 'starting'});
	try {
		if (previous) await previous.dispose();
		const {mountIsolated} = await import('./playground/runtime/index.js');
		if (current !== generation) return;
		const handle = await mountIsolated(element('terminal'), {
			moduleUrl: new URL('./playground/hunky.js', import.meta.url),
			wasmUrl: new URL('./playground/hunky_bg.wasm', import.meta.url),
			runtimeBaseUrl: new URL('./playground/runtime/', import.meta.url),
			columns: 100, rows: 30,
			onStatus: next => { if (current === generation) renderStatus(next); },
		});
		if (current !== generation) { await handle.dispose(); return; }
		runtime = handle;
		await inspect(handle, true);
	} catch (error) {
		if (current === generation) renderStatus({state: 'error', error: error.message});
	}
}

async function perform(operation) {
	busy = true;
	renderStatus();
	try {
		await operation(runtime);
		await inspect();
	} catch (error) {
		renderStatus({state: 'error', error: error.message});
	} finally {
		busy = false;
		renderStatus();
	}
}

const command = value => perform(handle => handle.send({type: 'paste', text: JSON.stringify(value)}));
document.querySelectorAll('[data-key]').forEach(button => {
	button.addEventListener('click', () => perform(async handle => {
		await handle.send({
			type: 'key', key: button.dataset.key, code: '', repeat: false,
			modifiers: {ctrl: false, alt: false, shift: false, meta: false},
		});
		handle.focus();
	}));
});
element('demo-reset').addEventListener('click', mount);
element('demo-load').addEventListener('click', () => perform(handle => inspect(handle, true)));
element('demo-inspect').addEventListener('click', () => perform(async () => {}));
element('demo-save').addEventListener('click', () => command({action: 'edit', path: element('demo-path').value, content: element('demo-content').value}));
element('demo-delete').addEventListener('click', () => command({action: 'delete', path: element('demo-path').value}));
element('demo-commit').addEventListener('click', () => command({action: 'commit', message: element('demo-commit-message').value}));
window.addEventListener('pagehide', () => {
	generation++;
	const previous = runtime;
	runtime = undefined;
	if (previous) void previous.dispose().catch(error => console.error('Playground cleanup failed', error));
});
window.addEventListener('pageshow', event => { if (event.persisted) void mount(); });
void mount();

---
sidebar_position: 4
---

# Architecture

## Project Structure

```
hunky/
├── src/
│   ├── main.rs      # Entry point and CLI parsing
│   ├── app.rs       # Main application logic and state
│   ├── git.rs       # Git operations (diff, status)
│   ├── diff.rs      # Diff data structures
│   ├── watcher.rs   # File system watcher
│   ├── syntax.rs    # Syntax highlighting
│   └── ui.rs        # TUI rendering with ratatui
├── Cargo.toml       # Rust dependencies
└── flake.nix        # Nix development environment
```

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI framework |
| `crossterm` | Terminal manipulation |
| `git2` | Git operations via libgit2 |
| `notify` | File system watching |
| `tokio` | Async runtime |
| `syntect` | Syntax highlighting |
| `similar` | Diff generation |
| `clap` | CLI argument parsing |

## Release versions

`version.txt` is the release version. The release workflow updates it together
with the Hunky package versions in `Cargo.toml` and `Cargo.lock` using
`node scripts/release-version.mjs --sync`. CI checks all three with `--check`.
Nix reads its package version from `Cargo.toml`, and `hunky --version` uses
Cargo's package version. Release packaging verifies that each binary reports
the version being published.

An unpinned `nix run github:randymarsh77/hunky` builds the source revision resolved
by Nix, not necessarily the latest release tag. To select a particular release,
use a tag such as `nix run github:randymarsh77/hunky/v0.2.2`; use `--refresh` to
refresh Nix's cached resolution of an unpinned reference. The package version
labels the build but does not select which source revision is built.

## Native end-to-end tests

`tests/e2e` runs the actual native binary in a Unix pseudo-terminal, with a
terminal emulator reading its rendered output and real keyboard events driving
the application. Each test creates an isolated repository using `git init`,
with independent Git configuration, commits and working-tree edits. Assertions
inspect exact index blobs and file modes, porcelain status, unchanged HEAD and
unchanged working-tree bytes rather than trusting the staged UI indicator.

The 44 scenarios cover file/hunk/line stage and unstage, restaging, multiple hunks
and files, duplicate lines, partially staged and index-only changes, untracked,
added/deleted/renamed files, unborn repositories, empty/binary files, ignored
files, special paths, newline formats, executable files, external watcher
updates, review, successful/canceled commit-editor handoff, index-lock error
reporting and retry, buffered/automatic mode transitions, focus/reset/resize,
repository discovery and invalid-path startup, splash skipping and terminal cleanup.
It runs on Linux in CI. Run locally on macOS or Linux:

```bash
nix develop --command cargo build --locked
python3 -m venv target/e2e-venv
target/e2e-venv/bin/pip install -r tests/e2e/requirements.txt
target/e2e-venv/bin/python -m unittest discover -s tests/e2e -v
```

Set `HUNKY_BINARY` to test a different native build and `HUNKY_E2E_TIMEOUT`
to adjust the default 10-second condition deadline on a slower machine.
Tests wait for terminal or Git state changes rather than fixed startup sleeps.
Failures retain ANSI transcripts, emulated screen contents and Git diffs under
`target/e2e-results`, uploaded by CI. No staging regressions are skipped or
marked as expected failures in this suite. Windows PTY support is not included.

## Data Flow

1. **Watcher** (`watcher.rs`) monitors the file system for changes
2. **Git** (`git.rs`) captures diffs when changes are detected
3. **Diff** (`diff.rs`) structures the raw diff data into hunks
4. **App** (`app.rs`) manages state, navigation, and mode transitions
5. **UI** (`ui.rs`) renders the TUI using ratatui

## Browser playground

The playground pins the published `tui2web` Rust crate and `@tui2web/runtime`
npm package at version `0.1.0`, with registry checksums in the lockfiles.
Package publication belongs to tui2web's GitHub Actions release workflow;
Hunky does not publish these packages. Future upgrades must verify both
registry artifacts and pass native and browser acceptance before merging for
the existing Cloudflare Pages landing deployment.

The playground adds no Hunky repository secrets: the existing Cloudflare Pages
project uses its GitHub integration. The native release pipeline references
`NIX_SIGNING_KEY` for its Nix cache signing; the separate documentation deployment
uses GitHub's `GITHUB_TOKEN`, `pages: write` and `id-token: write` permissions.
Configure `CARGO_REGISTRY_TOKEN` and `NPM_TOKEN`
only in the tui2web repository, following its release workflow documentation.

The native cache workflow uses `static-nix-cache/setup`, `save` and `deploy`
through the upstream-maintained floating `v1` tag. Keep all three actions on the
same major version for their shared artifact contract, and validate the release
matrix when upgrading. Registry Cargo dependencies use
their `Cargo.lock` checksums; Nix `outputHashes` are only for Git dependencies.

Every production GitHub Pages deployment includes the binary cache at
`https://randymarsh77.github.io/hunky/cache`. When no new release build is needed,
`deploy-cache` supplies an empty paths file to the deploy action so it regenerates
the static index from the existing `nix-cache` GitHub release instead of detecting
or uploading runner store paths. A failed cache job blocks production Pages
deployment; pull request previews may omit the cache.

After Docusaurus builds, `site/scripts/assemble-nix-cache.mjs` validates and adds
`nix-cache-info` and the `.narinfo` files under `site/build/cache`. It rewrites
relative `nar/` URLs to absolute GitHub release downloads because GitHub Pages
does not support the generator's Cloudflare `_redirects` file. Store metadata
and signatures remain unchanged. Missing, empty or invalid cache snapshots fail
the deployment rather than publishing documentation that removes the cache.

The [landing page](https://hunky.sh), sourced from `landing/index.html`, embeds
the real Hunky application compiled ahead of time to
WebAssembly. Native and browser builds share `App::handle_key`, navigation,
selection, review state, syntax highlighting and `UI::draw`. The default
`native` feature retains crossterm, the filesystem watcher, libgit2 and the
external Git/editor integration. The `browser` feature replaces only these
platform boundaries with tui2web's in-memory repository and Worker host.

Every instance starts with an independent HEAD, index, working tree and two
simulated history entries. The fixture contains staged and unstaged changes,
an untracked file, a deletion and multiple separated hunks. Staging changes the
index, never the working tree. Internal edit commands change the working tree,
never the index. Index-only hunks expose staged changes that subsequent working-tree edits
have removed. Hunky keeps the same hunk/line/file toggle navigation; history
review still uses the actual commit picker and acceptance UI.

This is an explicitly labeled text-only Git simulation, not an implementation
of Git's object database. There are no branches, remotes, credentials,
subprocesses, visitor repository access or external editors. Internal commit commands
record only the simulated index in local history. Unsupported commands and
invalid edits show errors instead of reporting success.

The original hero screenshot and its dimensions remain unchanged, with a centered
translucent **Try it live** button. No runtime, Worker, frame, application module
or WASM is fetched or instantiated until that button is activated. A reduced-motion
aware loading shimmer replaces the image, then the real terminal occupies its
original screen region with decorative CSS chrome and no layout shift.

After activation, the page loads self-hosted assets under `/playground/`, then mounts
an opaque-origin iframe (`sandbox="allow-scripts"`) through
`@tui2web/runtime`. Its prebuilt WASM runs in a bounded Worker with a
network-denying CSP. The parent provisions only the selected bundled assets.
No app code is compiled in the browser. There is no persistence: restart/reset
creates the original fixture, including HEAD/index/history, and unmount disposes
the Worker and iframe. Quitting with `Q` shows "Hunky exited. Restart to try again."
with a keyboard-accessible Restart link; failures provide a concise Retry link.
There are no public guide, editor, inspector or staging controls outside the TUI.
Semantic browser tests use an independent test-only runtime harness to inspect
complete repository snapshots without adding production UI or polling.

Build with `nix develop --command npm --prefix landing ci` followed by
`nix develop --command npm --prefix landing run build`. The locked Nix shell
provides the WASM Rust target and matching wasm-bindgen tooling. Run
`nix develop --command cargo test --no-default-features --features browser --lib`
for the adapter's semantic tests and `nix develop --command npm --prefix landing
test` for Chromium acceptance against the built site. Set `DEMO_URL` to a preview
or production URL to exercise the same acceptance suite against deployed assets.
On a fresh machine, install Playwright's Chromium using
`nix develop --command npm --prefix landing exec
-- playwright install chromium`.

The existing Cloudflare Pages project **hunky** serves `hunky.sh` and
`hunky.pages.dev` from its GitHub integration, with production branch `master`,
repository root unchanged, build command `bash landing/build-cloudflare.sh`
and output directory `landing/dist`. The script installs pinned Rust 1.93.1 and
the WASM target, installs the wasm-bindgen CLI matching `Cargo.lock`, then runs
`npm ci` and the static build. Compilation happens only in CI. The output includes
the page, precompiled WASM, self-hosted JavaScript/CSS and third-party licenses;
no runtime server or guest network access is required.

Validate both the local build and a Cloudflare Git-build preview before merging
landing changes. When migrating deployment settings, retain the previous settings
and deployment ID for rollback, and never switch old production source to an
unavailable build script. The initial transition uses an explicit legacy-screenshot
build only while older source lacks the script; an existing script's failure is
never replaced by that legacy output. Remove that compatibility command after
the production branch contains the script and live interaction is verified.

The separate GitHub Pages `deploy-site` workflow continues to deploy Docusaurus
documentation, coverage and benchmarks. It does not host the landing playground.

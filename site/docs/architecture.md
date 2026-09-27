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

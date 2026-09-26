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

**Release status:** this review branch temporarily pins immutable public Git
revisions of tui2web. Package publication belongs to the tui2web repository's
GitHub Actions release workflow, using its configured publishing secrets, not
local npm or Cargo login. Wait for successful publication and registry
verification of both the Rust crate and npm runtime from the intended immutable
release tag. Only then replace both pins with the validated published versions,
update the lockfiles, remove the temporary Nix Git `outputHashes`, and rerun
native and browser acceptance before opening the normal PR and merging for
the existing Pages deployment. Do not trigger a preview or production deployment
before this gate; Hunky does not publish the tui2web packages.

The playground adds no Hunky repository secrets. The existing native release
pipeline references `NIX_SIGNING_KEY` for its Nix cache signing; GitHub supplies
`GITHUB_TOKEN`, and the Pages deployment uses the workflow's `pages: write` and
`id-token: write` permissions. Configure `CARGO_REGISTRY_TOKEN` and `NPM_TOKEN`
only in the tui2web repository, following its release workflow documentation.

The [homepage](/) embeds the real Hunky application compiled ahead of time to
WebAssembly. Native and browser builds share `App::handle_key`, navigation,
selection, review state, syntax highlighting and `UI::draw`. The default
`native` feature retains crossterm, the filesystem watcher, libgit2 and the
external Git/editor integration. The `browser` feature replaces only these
platform boundaries with tui2web's in-memory repository and Worker host.

Every instance starts with an independent HEAD, index, working tree and two
simulated history entries. The fixture contains staged and unstaged changes,
an untracked file, a deletion and multiple separated hunks. Staging changes the
index, never the working tree. The editor changes the working tree, never the
index. Index-only hunks expose staged changes that subsequent working-tree edits
have removed. Hunky keeps the same hunk/line/file toggle navigation; history
review still uses the actual commit picker and acceptance UI.

This is an explicitly labeled text-only Git simulation, not an implementation
of Git's object database. There are no branches, remotes, credentials,
subprocesses, visitor repository access or external editors. The Commit button
records only the simulated index in local history. Unsupported commands and
invalid edits show errors instead of reporting success.

The landing page loads self-hosted assets under `/hunky/playground/`, then mounts
an opaque-origin iframe (`sandbox="allow-scripts"`) through
`@tui2web/runtime`. Its prebuilt WASM runs in a bounded Worker with a
network-denying CSP. The parent provisions only the selected bundled assets.
No app code is compiled in the browser. There is no persistence: restart/reset
creates the original fixture, including HEAD/index/history, and unmount disposes
the Worker and iframe. The inspector reads a complete repository snapshot.

Build with `nix develop --command npm --prefix site ci` followed by
`nix develop --command npm --prefix site run build`. The locked Nix shell
provides the WASM Rust target and matching wasm-bindgen tooling. Run
`nix develop --command cargo test --no-default-features --features browser --lib`
for the adapter's semantic tests and `nix develop --command npm --prefix site run
test:demo` for Chromium acceptance against the built site. On a fresh machine,
install Playwright's Chromium using `nix develop --command npm --prefix site exec
-- playwright install chromium`.

The existing GitHub Pages `deploy-site` workflow deploys the Docusaurus build,
including docs, coverage, benchmarks and the playground. Browser acceptance is
a deployment prerequisite; neither the unused `landing/` directory nor an
additional hosting provider is involved.

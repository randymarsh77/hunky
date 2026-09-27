# Hunky

A terminal UI that watches a git repository and shows each diff as it lands.
Run it next to a coding agent to see what it's changing, stage what you want,
and commit.

**[Try it in your browser](https://hunky.sh)** · **[Docs](https://randymarsh77.github.io/hunky/docs/intro)**

## Highlights

- **Live diffs.** Hunky watches the working tree and updates as files change. No refresh.
- **Stream only what's new.** Streaming mode hides everything that existed when you
  started and shows new hunks one at a time, stepped manually or auto-advanced.
- **Stage hunks or single lines.** `S` stages the current hunk; `L` switches to line
  mode for partial staging. `C` opens your git editor to commit.
- **Review recent commits.** `R` picks a commit and walks its hunks so you can mark
  each one reviewed.

## Install

```bash
# Nix
nix run github:randymarsh77/hunky

# Prebuilt binaries (macOS, Linux, Windows)
# https://github.com/randymarsh77/hunky/releases

# From source
cargo install --git https://github.com/randymarsh77/hunky
```

## Use

```bash
hunky                 # watch the current repository
hunky --repo <path>   # watch another one
hunky --no-splash     # skip the startup animation
```

Press `H` for key bindings. See the [docs](https://randymarsh77.github.io/hunky/docs/intro)
for the full reference, the demo script, and development setup.

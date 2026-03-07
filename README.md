<div align="center">

# chess2lichess (`c2l`)

**Turn a chess.com game URL into a lichess analysis URL from your terminal.**

[![Rust](https://img.shields.io/badge/Rust-2024_edition-000000?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Build](https://img.shields.io/badge/build-cargo%20build-blue?style=flat-square)](#build)

[Overview](#overview) • [Install](#install) • [Usage](#usage) • [TUI](#tui-mode) • [Limits](#limits)

</div>

## Overview

`c2l` is a Rust CLI/TUI tool that:

1. validates a `chess.com/game/live/...` URL,
2. extracts PGN from chess.com (callback/page/archive fallbacks),
3. imports that PGN to `https://lichess.org/api/import`,
4. returns the final lichess analysis URL.

It is designed for fast, keyboard-first game analysis handoff.

> [!IMPORTANT]
> This tool currently supports **chess.com live game URLs only**.

## Features

- URL validation for supported chess.com game links
- PGN extraction with multiple fallback strategies
- Lichess import via official API endpoint
- Optional PGN copy / print / save
- Optional browser auto-open
- Interactive terminal mode and TUI mode

## Install

### npm (recommended)

```bash
npm install -g @wnsdud-jy/c2l
```

> [!NOTE]
> `npm` install downloads a platform-specific prebuilt `c2l` binary from GitHub Releases.

### Build from source

```bash
cargo build --release
```

Binary path:

```bash
target/release/c2l
```

### Run without install

```bash
cargo run -- "https://www.chess.com/game/live/123456789"
```

## Usage

```text
Usage: c2l [OPTIONS] [URL] [COMMAND]

Commands:
  tui   Run TUI mode
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [URL]  chess.com game URL

Options:
      --copy             Copy PGN to clipboard
      --open             Open browser
      --print-pgn        Print PGN to stdout
      --no-open          Do not open browser automatically
      --save-pgn <PATH>  Save PGN to file
      --raw-url          Print only the final URL
  -h, --help             Print help
  -V, --version          Print version
```

### Examples

Direct URL:

```bash
c2l "https://www.chess.com/game/live/123456789"
```

Output final URL only:

```bash
c2l --raw-url "https://www.chess.com/game/live/123456789"
```

Save PGN and keep browser closed:

```bash
c2l --save-pgn game.pgn --no-open "https://www.chess.com/game/live/123456789"
```

Prompt for URL interactively:

```bash
c2l
```
Then enter a URL and it will run continuously until you type `q`, `quit`, `exit`, or press Ctrl+C.
Example session:

```bash
c2l
URL> https://www.chess.com/game/live/123
URL> https://www.chess.com/game/live/456
URL> quit
```

> [!TIP]
> Use `--raw-url` when chaining with shell tools, for example: `c2l --raw-url <url> | xargs -n1 echo`.

## TUI Mode

Start TUI:

```bash
c2l tui
```

Keybindings:

- `Enter`: process current URL
- `c`: copy PGN to clipboard
- `o`: open final URL
- `p`: save PGN to `c2l-last.pgn`
- `q`: quit TUI

TUI runs in session mode as well: after a URL is processed, it keeps the input ready for the next URL.

### TUI color troubleshooting

If colors look unchanged or plain, run in a terminal that supports color:

- `TERM` should be `xterm-256color` or compatible
- set `COLORTERM=truecolor` when supported by your terminal

On limited terminals, output is still functional and only the color layer is reduced.


## How It Works

Processing stages:

1. URL parse and support check
2. chess.com game data lookup
3. PGN extraction + PGN shape validation
4. lichess import API call
5. final analysis URL output

If enabled, `c2l` can also copy PGN, save PGN, print PGN, and open browser.

## Limits

- Non-chess.com URLs are rejected.
- Private/restricted games may fail to resolve.
- If chess.com page/API shape changes, PGN extraction may break.
- If lichess API behavior changes, final URL extraction may fail.

## npm Package Notes

- Supported npm binary targets:
  - `linux-x64`
  - `linux-arm64`
  - `darwin-x64`
  - `darwin-arm64`
  - `win32-x64`
- To skip binary download at install time: `C2L_SKIP_POSTINSTALL=1`
- To override release repository (advanced): `C2L_GITHUB_REPO=<owner/repo>`
- To override release tag (advanced): `C2L_RELEASE_TAG=vX.Y.Z`

If install fails, check that the matching release asset exists for your package version.

> [!NOTE]
> Clipboard and browser-open behavior depend on local OS/session capabilities.

## Development

Run tests:

```bash
cargo test
```

Format check:

```bash
cargo fmt -- --check
```

npm tests:

```bash
npm test
```

## Release (Maintainers)

1. Ensure `Cargo.toml` and `package.json` versions match.
2. Push a tag like `v0.1.1`.
3. `release-binaries` workflow uploads 5 platform binaries + checksum file to the GitHub Release.
4. `npm-publish` workflow publishes `@wnsdud-jy/c2l` publicly.

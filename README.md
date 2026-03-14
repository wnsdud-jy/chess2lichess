<div align="center">

# chess2lichess (`c2l`)

**Turn a chess.com game URL into a lichess analysis URL from the terminal.**

![npm version](https://img.shields.io/npm/v/@wnsdud-jy/c2l?style=flat-square)
![node](https://img.shields.io/node/v/@wnsdud-jy/c2l?style=flat-square)
![license](https://img.shields.io/badge/license-MIT-9c27b0?style=flat-square)

[Overview](#overview) • [Quick start](#quick-start---npm) • [Usage](#usage) • [TUI](#tui-mode) • [npm package notes](#npm-package-notes) • [Limits](#limits)

</div>

## Overview

`c2l` is a small CLI that converts a `chess.com` game URL into a `lichess.org` analysis link.

It runs as a native binary (Rust implementation) while providing a simple Node entry point at the package level.

> [!IMPORTANT]
> `c2l` supports `https://www.chess.com/game/<id>` and `https://www.chess.com/game/live/<id>` URLs.

## Key features

- URL validation for supported `chess.com/game/...` links
- PGN extraction with fallback strategies
- PGN import via `https://lichess.org/api/import`
- Final analysis URL output and optional actions
  - copy PGN to clipboard
  - print PGN
  - save PGN to a file
  - auto open browser
- Interactive URL input mode and TUI mode

## Quick start - npm

Install globally for shell usage:

```bash
npm install -g @wnsdud-jy/c2l
```

Use `c2l` directly:

```bash
c2l "https://www.chess.com/game/live/123456789"
```

Run without global install:

```bash
npx @wnsdud-jy/c2l "https://www.chess.com/game/live/123456789"
```

If you want a local dependency:

```bash
npm i -D @wnsdud-jy/c2l
npx c2l "https://www.chess.com/game/live/123456789"
```

> [!TIP]
> `npm`/`npx` install does **not** require a Rust toolchain. It downloads a prebuilt binary for your platform.

## Usage

```text
Usage: c2l [OPTIONS] [URL...] [COMMAND]

Commands:
  tui   Run TUI mode
  doctor  Run environment and release checks
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [URL]  chess.com game URL(s). Can be repeated for batch mode.

Options:
      --copy             Copy PGN to clipboard
      --open             Open browser
      --print-pgn        Print PGN to stdout
      --no-open          Do not open browser automatically
      --save-pgn <PATH>  Save PGN to file
      --raw-url          Print only the final URL
      --json             Print machine-readable JSON output
      --quiet            Suppress human-readable progress and summary messages
      --verbose          Show verbose progress logs
      --format <FORMAT>  Output format: text|json|csv [default: text]
      --input <PATH>     Read URLs from a file, one per line
  -h, --help             Print help
  -V, --version          Print version
```

Examples:

```bash
# Process one URL
c2l "https://www.chess.com/game/123456789"

# Process one live URL
c2l "https://www.chess.com/game/live/123456789"

# Process multiple URLs at once
c2l "https://www.chess.com/game/live/111" "https://www.chess.com/game/live/222" --raw-url

# Process file input and output JSON
c2l --json --input urls.txt

# Pipe URLs via stdin
printf "%s\n%s\n" "https://www.chess.com/game/live/111" "https://www.chess.com/game/live/222" | c2l --json

# Get only final URL for scripting
c2l --raw-url "https://www.chess.com/game/live/123456789"

# Save PGN and avoid opening browser
c2l --save-pgn game.pgn --no-open "https://www.chess.com/game/live/123456789"

# Interactive mode
c2l
URL> https://www.chess.com/game/live/111
URL> https://www.chess.com/game/live/222
URL> quit
```

> [!TIP]
> For shell pipelines, pass `--raw-url` and chain output directly.

## TUI mode

```bash
c2l tui
```

- `Enter`: process current URL
- `c`: copy PGN to clipboard
- `o`: open final URL in browser
- `p`: save PGN as `c2l-last.pgn`
- `q`, `Esc`, or `Ctrl+C`: quit

## npm package notes

During `npm` install, `postinstall` runs `scripts/postinstall.js`:

- Resolve your platform/arch (`linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`, `win32-x64`)
- Download the matching asset from GitHub Releases
- Verify SHA-256 via checksums file
- Save binary to `vendor/` in the package
- Make binary executable (`chmod +x`) on non-Windows platforms

Supported environment variables:

- `C2L_SKIP_POSTINSTALL=1`: skip download step
- `C2L_GITHUB_REPO=<owner/repo>`: override release repository
- `C2L_RELEASE_TAG=<tag>`: override release tag (default: `v<package-version>`)

`bin/c2l.js` simply launches the downloaded binary for your environment.

## Build from source

```bash
cargo build --release
cargo test
```

Run Node-side tests:

```bash
npm test
```

## Limits

- Non-chess.com URLs are rejected.
- Private/restricted games may fail to resolve.
- If chess.com markup/API shape changes, extraction can break.
- If lichess API behavior changes, final URL parsing can fail.
- Output color in TUI depends on terminal capability.

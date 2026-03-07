# c2l

`c2l` is a Rust CLI tool that converts a chess.com live game URL into a lichess analysis URL.

## Quick Start

```bash
cargo build --release
./target/release/c2l "https://www.chess.com/game/live/123456789"
```

## Notes

- Supports chess.com live game URLs
- Includes optional TUI mode via `c2l tui`

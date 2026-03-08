# Changelog

## [Unreleased]

- No changes yet.

## [0.1.4] - 2026-03-08

### Added
- Added MIT license and version metadata updates for 0.1.4 release.
- Added default gradient animation to the interactive `URL>` prompt.
- Improved interactive CLI UX by removing color from top instruction line.
- Reworked terminal progress output into muted-step flow (`1/5`, `2/5`, ...) with gradient animated `Working...`.
- Highlighted failed conversion counts in red in summary output.

### Changed
- Adjusted retry and output flow while preserving behavior for JSON/CSV/raw outputs.
- Kept `fetch_game_pgn` behavior stable by reverting the previous extraction regression path.
- Updated TUI progress/event wiring for new non-callback progress flow.

### Fixed
- Fixed interactive mode behavior so prompt animation continuously runs while waiting for input.
- Removed an unused TUI event variant (`WorkerEvent::Log`) and aligned event handling.

## [0.1.3]

- Initial baseline prior to 0.1.4 updates.


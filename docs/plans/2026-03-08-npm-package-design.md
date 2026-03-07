# @wnsdud/c2l npm Packaging Design

Date: 2026-03-08
Status: Approved

## Goal

Package the existing Rust CLI (`c2l`) as a public npm package named `@wnsdud/c2l` so users can install and run it without local Rust toolchain builds.

## Confirmed Requirements

- Package name: `@wnsdud/c2l`
- npm visibility: `public`
- Install strategy: prebuilt binaries only (no local build fallback)
- Supported targets:
  - `linux-x64`
  - `linux-arm64`
  - `darwin-x64`
  - `darwin-arm64`
  - `win32-x64`
- GitHub Actions must attach per-platform binaries automatically on release

## Architecture

1. Keep current Rust application architecture unchanged (`src/*`)
2. Add npm wrapper layer:
   - `package.json`: npm metadata, `bin` entry, `postinstall` hook
   - `bin/c2l.js`: node launcher for installed binary
   - `scripts/postinstall.js`: download and install matching release binary
3. Store prebuilt binaries in GitHub Releases
4. Keep version alignment between Cargo package and npm package

## Release and Distribution Flow

1. Push tag `vX.Y.Z`
2. GitHub Actions matrix builds all 5 targets
3. Workflow archives binaries and uploads them to the GitHub Release
4. Workflow also uploads SHA256 checksum file
5. npm package is published as `@wnsdud/c2l` (public)
6. During npm install, `postinstall` downloads matching asset and verifies checksum

## Error Handling

- Unsupported OS/arch:
  - print current platform details and supported target list
- Missing release asset:
  - print selected version/tag and attempted URL
- Network/download failure:
  - retry up to 2 times, then fail with clear message
- Archive extraction/permission failure:
  - print file path and root error
- Launcher execution failure:
  - pass through child process exit code and stdio
- Integrity failure:
  - fail hard when checksum verification does not match

## Testing Strategy

### Local

- `cargo build --release`
- `node scripts/postinstall.js --dry-run`
- `node bin/c2l.js --version`

### CI

- Matrix build completes for all 5 targets
- Archive naming convention validated
- Checksum generation and upload validated

### Release Verification

- Confirm 5 platform assets + checksum file attached to release
- Fresh environment install smoke test:
  - `npm i -g @wnsdud/c2l`
  - `c2l --help`

## Non-Goals

- JS reimplementation of chess logic
- Native Node.js addon bindings
- Local source build fallback during npm install

## Follow-up

Create an implementation plan and then implement:
- npm wrapper files
- release build/upload workflows
- install-time downloader and verifier

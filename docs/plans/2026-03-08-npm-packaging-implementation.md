# @wnsdud/c2l npm Packaging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship `@wnsdud/c2l` as a public npm package that installs and runs a platform-specific prebuilt `c2l` binary from GitHub Releases.

**Architecture:** Keep Rust CLI sources unchanged and add a thin Node.js installer/launcher wrapper. Release binaries are produced by a GitHub Actions matrix and uploaded with checksum metadata; npm `postinstall` resolves target platform, downloads the binary, verifies SHA256, and stores it in a package-local runtime path.

**Tech Stack:** Rust (cargo), Node.js (built-in `https`, `crypto`, `child_process`, `node:test`), npm, GitHub Actions.

---

Related skills for execution: `@build-fix`, `@git-master`, `@ultraqa`.

### Task 1: Scaffold npm wrapper metadata and platform resolver

**Files:**
- Create: `package.json`
- Create: `bin/c2l.js`
- Create: `scripts/lib/platform.js`
- Create: `tests/platform.test.mjs`
- Modify: `README.md`

**Step 1: Write the failing test**

```js
// tests/platform.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveTarget, buildAssetName } from '../scripts/lib/platform.js';

test('resolveTarget maps darwin arm64', () => {
  assert.equal(resolveTarget('darwin', 'arm64'), 'darwin-arm64');
});

test('buildAssetName uses version and target', () => {
  assert.equal(buildAssetName('0.1.0', 'linux-x64'), 'c2l-v0.1.0-linux-x64');
});
```

**Step 2: Run test to verify it fails**

Run: `node --test tests/platform.test.mjs`
Expected: FAIL with module-not-found for `scripts/lib/platform.js`

**Step 3: Write minimal implementation**

```js
// scripts/lib/platform.js
export const SUPPORTED_TARGETS = new Set([
  'linux-x64',
  'linux-arm64',
  'darwin-x64',
  'darwin-arm64',
  'win32-x64',
]);

export function resolveTarget(platform, arch) {
  const target = `${platform}-${arch}`;
  if (!SUPPORTED_TARGETS.has(target)) {
    throw new Error(`Unsupported platform/arch: ${platform}/${arch}`);
  }
  return target;
}

export function buildAssetName(version, target) {
  const suffix = target === 'win32-x64' ? '.exe' : '';
  return `c2l-v${version}-${target}${suffix}`;
}
```

```json
// package.json (initial)
{
  "name": "@wnsdud/c2l",
  "version": "0.1.0",
  "description": "Convert chess.com live game URL to lichess analysis URL",
  "license": "MIT",
  "bin": {
    "c2l": "bin/c2l.js"
  },
  "type": "module",
  "scripts": {
    "postinstall": "node scripts/postinstall.js",
    "test": "node --test tests/*.test.mjs"
  }
}
```

```js
// bin/c2l.js (placeholder until Task 3)
#!/usr/bin/env node
console.error('c2l launcher not implemented yet');
process.exit(1);
```

**Step 4: Run test to verify it passes**

Run: `node --test tests/platform.test.mjs`
Expected: PASS

**Step 5: Commit**

```bash
git add package.json scripts/lib/platform.js tests/platform.test.mjs bin/c2l.js README.md
git commit -m "feat(npm): add package scaffold and platform mapping"
```

### Task 2: Implement downloader, checksum verification, and installer

**Files:**
- Create: `scripts/postinstall.js`
- Create: `scripts/lib/checksum.js`
- Create: `tests/checksum.test.mjs`
- Modify: `scripts/lib/platform.js`

**Step 1: Write the failing test**

```js
// tests/checksum.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseChecksums, matchChecksum } from '../scripts/lib/checksum.js';

test('parseChecksums reads sha lines', () => {
  const parsed = parseChecksums('abc123  c2l-v0.1.0-linux-x64\n');
  assert.equal(parsed.get('c2l-v0.1.0-linux-x64'), 'abc123');
});

test('matchChecksum throws when missing entry', () => {
  const map = new Map();
  assert.throws(() => matchChecksum(map, 'c2l-v0.1.0-linux-x64'), /Missing checksum/);
});
```

**Step 2: Run test to verify it fails**

Run: `node --test tests/checksum.test.mjs`
Expected: FAIL with module-not-found for `scripts/lib/checksum.js`

**Step 3: Write minimal implementation**

```js
// scripts/lib/checksum.js
export function parseChecksums(raw) {
  const map = new Map();
  for (const line of raw.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const [hash, filename] = trimmed.split(/\s+/);
    map.set(filename, hash.toLowerCase());
  }
  return map;
}

export function matchChecksum(checksums, filename) {
  const hash = checksums.get(filename);
  if (!hash) throw new Error(`Missing checksum for ${filename}`);
  return hash;
}
```

```js
// scripts/postinstall.js (core behavior)
// 1) resolve platform target
// 2) download binary + checksum file from GitHub release
// 3) verify SHA256
// 4) write installed binary to vendor/c2l(.exe)
// 5) chmod +x on non-windows
```

**Step 4: Run tests and dry-run installer**

Run: `node --test tests/checksum.test.mjs tests/platform.test.mjs`
Expected: PASS

Run: `node scripts/postinstall.js --dry-run`
Expected: prints resolved release URLs and destination path without downloading

**Step 5: Commit**

```bash
git add scripts/postinstall.js scripts/lib/checksum.js scripts/lib/platform.js tests/checksum.test.mjs
git commit -m "feat(npm): add binary downloader with checksum verification"
```

### Task 3: Implement launcher execution and missing-binary diagnostics

**Files:**
- Modify: `bin/c2l.js`
- Create: `scripts/lib/paths.js`
- Create: `tests/paths.test.mjs`

**Step 1: Write the failing test**

```js
// tests/paths.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { binaryFilename } from '../scripts/lib/paths.js';

test('binaryFilename returns exe on win32', () => {
  assert.equal(binaryFilename('win32'), 'c2l.exe');
});
```

**Step 2: Run test to verify it fails**

Run: `node --test tests/paths.test.mjs`
Expected: FAIL with module-not-found for `scripts/lib/paths.js`

**Step 3: Write minimal implementation**

```js
// scripts/lib/paths.js
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
export const packageRoot = path.resolve(here, '..', '..');

export function binaryFilename(platform) {
  return platform === 'win32' ? 'c2l.exe' : 'c2l';
}

export function installedBinaryPath(platform) {
  return path.join(packageRoot, 'vendor', binaryFilename(platform));
}
```

```js
// bin/c2l.js
#!/usr/bin/env node
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import { installedBinaryPath } from '../scripts/lib/paths.js';

const binPath = installedBinaryPath(process.platform);
if (!fs.existsSync(binPath)) {
  console.error('c2l binary not found. Reinstall @wnsdud/c2l for your platform.');
  process.exit(1);
}

const child = spawn(binPath, process.argv.slice(2), { stdio: 'inherit' });
child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
```

**Step 4: Run tests and launcher smoke check**

Run: `node --test tests/paths.test.mjs tests/platform.test.mjs tests/checksum.test.mjs`
Expected: PASS

Run: `node bin/c2l.js --version`
Expected: Either prints c2l version (if binary installed) or a clear missing-binary message

**Step 5: Commit**

```bash
git add bin/c2l.js scripts/lib/paths.js tests/paths.test.mjs
git commit -m "feat(npm): add launcher for installed c2l binary"
```

### Task 4: Add GitHub Actions release workflow for platform binaries

**Files:**
- Create: `.github/workflows/release-binaries.yml`
- Create: `.github/workflows/npm-publish.yml`

**Step 1: Write failing validation step**

Add workflow step that fails if `package.json` version and `Cargo.toml` version differ.

```bash
cargo_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
npm_version=$(node -p "require('./package.json').version")
[ "$cargo_version" = "$npm_version" ]
```

**Step 2: Run local syntax check**

Run: `node -e "JSON.parse(require('fs').readFileSync('package.json','utf8'))"`
Expected: no error

**Step 3: Write minimal implementation**

```yaml
# .github/workflows/release-binaries.yml
name: release-binaries
on:
  push:
    tags: ['v*']
permissions:
  contents: write
jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            platform: linux
            arch: x64
            exe_suffix: ''
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            platform: linux
            arch: arm64
            exe_suffix: ''
          - os: macos-13
            target: x86_64-apple-darwin
            platform: darwin
            arch: x64
            exe_suffix: ''
          - os: macos-14
            target: aarch64-apple-darwin
            platform: darwin
            arch: arm64
            exe_suffix: ''
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            platform: win32
            arch: x64
            exe_suffix: .exe
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
      - run: node .github/workflows/scripts/package-binary.mjs
```

```yaml
# .github/workflows/npm-publish.yml
name: npm-publish
on:
  workflow_dispatch:
permissions:
  contents: read
  id-token: write
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          registry-url: https://registry.npmjs.org
      - run: npm publish --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

**Step 4: Validate workflow files**

Run: `git diff -- .github/workflows`
Expected: matrix includes exactly 5 platforms and checksum upload steps

**Step 5: Commit**

```bash
git add .github/workflows/release-binaries.yml .github/workflows/npm-publish.yml
git commit -m "ci: build and upload platform binaries on release tags"
```

### Task 5: Documentation and end-to-end smoke checks

**Files:**
- Modify: `README.md`

**Step 1: Write failing docs checklist**

Create checklist in commit message draft and fail CI if README misses `npm install -g @wnsdud/c2l`.

**Step 2: Run checks before editing**

Run: `rg "npm install -g @wnsdud/c2l" README.md`
Expected: no match

**Step 3: Write minimal documentation update**

Add sections:
- npm install and npx usage
- release prerequisite (`vX.Y.Z` tag first)
- troubleshooting for unsupported platform or missing asset

**Step 4: Run smoke checks**

Run: `npm test`
Expected: PASS

Run: `node scripts/postinstall.js --dry-run`
Expected: resolved release URLs and install path printed

Run: `npm pack --dry-run`
Expected: includes `bin/`, `scripts/`, `vendor` target path placeholder, and excludes Rust build artifacts

**Step 5: Commit**

```bash
git add README.md
git commit -m "docs: add npm install and release troubleshooting guide"
```

### Task 6: Final release rehearsal

**Files:**
- Modify: `.github/workflows/release-binaries.yml` (if fixes discovered)
- Modify: `.github/workflows/npm-publish.yml` (if fixes discovered)

**Step 1: Run full local checks**

Run: `cargo build --release`
Expected: PASS

Run: `npm test`
Expected: PASS

**Step 2: Verify package metadata**

Run: `node -e "const p=require('./package.json'); console.log(p.name,p.version,p.bin.c2l)"`
Expected: `@wnsdud/c2l <version> bin/c2l.js`

**Step 3: Reconcile versions and changelog note**

Ensure `Cargo.toml` version equals `package.json` version before tagging.

**Step 4: Create release tag procedure**

Run:
```bash
git tag v0.1.0
git push origin v0.1.0
```
Expected: `release-binaries` workflow uploads 5 binaries + checksum file

**Step 5: Commit any final CI adjustments**

```bash
git add .github/workflows package.json README.md
git commit -m "chore: finalize npm release pipeline for @wnsdud/c2l"
```

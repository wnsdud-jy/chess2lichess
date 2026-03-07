import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildAssetName,
  buildChecksumsName,
  normalizeVersion,
  resolveTarget,
} from '../scripts/lib/platform.js';

test('resolveTarget maps darwin/arm64', () => {
  assert.equal(resolveTarget('darwin', 'arm64'), 'darwin-arm64');
});

test('resolveTarget rejects unsupported platform', () => {
  assert.throws(
    () => resolveTarget('win32', 'arm64'),
    /Unsupported platform\/arch: win32\/arm64/
  );
});

test('buildAssetName adds .exe for win32 target', () => {
  assert.equal(buildAssetName('0.1.0', 'win32-x64'), 'c2l-v0.1.0-win32-x64.exe');
});

test('buildChecksumsName uses normalized version', () => {
  assert.equal(buildChecksumsName('v0.1.0'), 'c2l-v0.1.0-checksums.txt');
});

test('normalizeVersion removes v prefix', () => {
  assert.equal(normalizeVersion('v2.3.4'), '2.3.4');
});

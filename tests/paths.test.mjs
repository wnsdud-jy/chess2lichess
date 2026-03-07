import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import { binaryFilename, installedBinaryPath } from '../scripts/lib/paths.js';

test('binaryFilename returns .exe for win32 target', () => {
  assert.equal(binaryFilename('win32-x64'), 'c2l.exe');
});

test('binaryFilename returns no suffix on unix target', () => {
  assert.equal(binaryFilename('linux-arm64'), 'c2l');
});

test('installedBinaryPath uses vendor directory', () => {
  const binPath = installedBinaryPath('darwin-x64');
  assert.equal(binPath.endsWith(path.join('vendor', 'c2l')), true);
});

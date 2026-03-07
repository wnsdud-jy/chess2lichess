import assert from 'node:assert/strict';
import test from 'node:test';

import { matchChecksum, parseChecksums } from '../scripts/lib/checksum.js';

test('parseChecksums parses sha256 lines', () => {
  const raw =
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  c2l-v0.1.0-linux-x64\n';

  const map = parseChecksums(raw);

  assert.equal(
    map.get('c2l-v0.1.0-linux-x64'),
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
  );
});

test('parseChecksums skips invalid lines', () => {
  const map = parseChecksums('not-a-checksum-line\n');
  assert.equal(map.size, 0);
});

test('matchChecksum throws for missing asset', () => {
  assert.throws(
    () => matchChecksum(new Map(), 'c2l-v0.1.0-linux-x64'),
    /Missing checksum entry/
  );
});

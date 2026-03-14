import test from 'node:test';
import assert from 'node:assert/strict';

import { buildChildEnv } from '../scripts/lib/launch.js';

test('buildChildEnv forwards npm wrapper metadata', () => {
  const env = buildChildEnv(
    { PATH: '/tmp/bin' },
    { name: '@wnsdud-jy/c2l', version: '0.1.6' }
  );

  assert.equal(env.PATH, '/tmp/bin');
  assert.equal(env.C2L_NPM_WRAPPER, '1');
  assert.equal(env.C2L_NPM_PACKAGE_NAME, '@wnsdud-jy/c2l');
  assert.equal(env.C2L_NPM_PACKAGE_VERSION, '0.1.6');
});

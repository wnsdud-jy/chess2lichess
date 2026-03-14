#!/usr/bin/env node
import fs from 'node:fs';
import { spawn } from 'node:child_process';

import { installedBinaryPath } from '../scripts/lib/paths.js';
import { resolveTarget } from '../scripts/lib/platform.js';
import { buildChildEnv } from '../scripts/lib/launch.js';

let target;

try {
  target = resolveTarget();
} catch (error) {
  console.error(`[c2l] ${error.message}`);
  process.exit(1);
}

const binPath = installedBinaryPath(target);

if (!fs.existsSync(binPath)) {
  console.error('[c2l] binary not found for this package install.');
  console.error(`[c2l] expected path: ${binPath}`);
  console.error('[c2l] try reinstalling: npm install -g @wnsdud-jy/c2l');
  process.exit(1);
}

const child = spawn(binPath, process.argv.slice(2), {
  stdio: 'inherit',
  env: buildChildEnv(),
});

child.on('error', (error) => {
  console.error(`[c2l] failed to execute binary: ${error.message}`);
  process.exit(1);
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});

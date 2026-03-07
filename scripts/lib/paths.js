import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { getTargetMeta, resolveTarget } from './platform.js';

const here = path.dirname(fileURLToPath(import.meta.url));

export const packageRoot = path.resolve(here, '..', '..');

export function binaryFilename(target = resolveTarget()) {
  const { exeSuffix } = getTargetMeta(target);
  return `c2l${exeSuffix}`;
}

export function installedBinaryPath(target = resolveTarget()) {
  return path.join(packageRoot, 'vendor', binaryFilename(target));
}

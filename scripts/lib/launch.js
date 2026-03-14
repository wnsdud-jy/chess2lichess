import fs from 'node:fs';
import path from 'node:path';

import { packageRoot } from './paths.js';

export function readPackageMetadata(root = packageRoot) {
  const packageJsonPath = path.join(root, 'package.json');
  return JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
}

export function buildChildEnv(baseEnv = process.env, packageJson = readPackageMetadata()) {
  return {
    ...baseEnv,
    C2L_NPM_WRAPPER: '1',
    C2L_NPM_PACKAGE_NAME: packageJson.name,
    C2L_NPM_PACKAGE_VERSION: packageJson.version,
  };
}

import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import https from 'node:https';
import path from 'node:path';

import { matchChecksum, parseChecksums } from './lib/checksum.js';
import {
  buildAssetName,
  buildChecksumsName,
  normalizeVersion,
  resolveTarget,
} from './lib/platform.js';
import { installedBinaryPath, packageRoot } from './lib/paths.js';

const MAX_REDIRECTS = 5;
const MAX_ATTEMPTS = 3;
const REQUEST_TIMEOUT_MS = 30_000;

async function readPackageJson() {
  const packageJsonPath = path.join(packageRoot, 'package.json');
  const raw = await fs.readFile(packageJsonPath, 'utf8');
  return JSON.parse(raw);
}

function requestBuffer(url, redirectCount = 0) {
  return new Promise((resolve, reject) => {
    const req = https.get(
      url,
      {
        headers: {
          'User-Agent': '@wnsdud/c2l-postinstall',
        },
      },
      (response) => {
        const statusCode = response.statusCode ?? 0;

        if (
          [301, 302, 303, 307, 308].includes(statusCode) &&
          response.headers.location
        ) {
          if (redirectCount >= MAX_REDIRECTS) {
            reject(new Error(`Too many redirects while requesting ${url}`));
            return;
          }

          const nextUrl = new URL(response.headers.location, url).toString();
          response.resume();
          resolve(requestBuffer(nextUrl, redirectCount + 1));
          return;
        }

        if (statusCode !== 200) {
          response.resume();
          reject(new Error(`Request failed for ${url}: HTTP ${statusCode}`));
          return;
        }

        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => resolve(Buffer.concat(chunks)));
        response.on('error', (error) => reject(error));
      }
    );

    req.setTimeout(REQUEST_TIMEOUT_MS, () => {
      req.destroy(new Error(`Timeout after ${REQUEST_TIMEOUT_MS}ms for ${url}`));
    });

    req.on('error', (error) => reject(error));
  });
}

async function withRetries(label, task) {
  let lastError;

  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
    try {
      return await task();
    } catch (error) {
      lastError = error;
      if (attempt < MAX_ATTEMPTS) {
        console.warn(
          `[c2l] ${label} failed (${attempt}/${MAX_ATTEMPTS}): ${error.message}`
        );
      }
    }
  }

  throw lastError;
}

function toSha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

async function installBinary({
  assetUrl,
  checksumsUrl,
  assetName,
  destinationPath,
  dryRun,
}) {
  if (dryRun) {
    console.log(`[c2l] dry-run target binary: ${assetName}`);
    console.log(`[c2l] dry-run asset url: ${assetUrl}`);
    console.log(`[c2l] dry-run checksums url: ${checksumsUrl}`);
    console.log(`[c2l] dry-run install path: ${destinationPath}`);
    return;
  }

  const binaryBuffer = await withRetries('binary download', () =>
    requestBuffer(assetUrl)
  );
  const checksumsBuffer = await withRetries('checksum download', () =>
    requestBuffer(checksumsUrl)
  );

  const checksums = parseChecksums(checksumsBuffer.toString('utf8'));
  const expectedHash = matchChecksum(checksums, assetName);
  const actualHash = toSha256(binaryBuffer);

  if (actualHash !== expectedHash) {
    throw new Error(
      `Checksum mismatch for ${assetName}. expected=${expectedHash} actual=${actualHash}`
    );
  }

  await fs.mkdir(path.dirname(destinationPath), { recursive: true });
  await fs.writeFile(destinationPath, binaryBuffer);

  if (process.platform !== 'win32') {
    await fs.chmod(destinationPath, 0o755);
  }

  console.log(`[c2l] installed binary to ${destinationPath}`);
}

async function main() {
  if (process.env.C2L_SKIP_POSTINSTALL === '1') {
    console.log('[c2l] skipping postinstall (C2L_SKIP_POSTINSTALL=1)');
    return;
  }

  const dryRun = process.argv.includes('--dry-run');
  const packageJson = await readPackageJson();

  const version = normalizeVersion(packageJson.version);
  const tag = process.env.C2L_RELEASE_TAG || `v${version}`;
  const repo = process.env.C2L_GITHUB_REPO || packageJson.c2l?.githubRepo;

  if (!repo) {
    throw new Error('GitHub repository is not configured for release downloads.');
  }

  const target = resolveTarget();
  const assetName = buildAssetName(version, target);
  const checksumsName = buildChecksumsName(version);
  const baseUrl = `https://github.com/${repo}/releases/download/${tag}`;
  const destinationPath = installedBinaryPath(target);

  const assetUrl = `${baseUrl}/${assetName}`;
  const checksumsUrl = `${baseUrl}/${checksumsName}`;

  await installBinary({
    assetUrl,
    checksumsUrl,
    assetName,
    destinationPath,
    dryRun,
  });
}

main().catch((error) => {
  console.error(`[c2l] postinstall failed: ${error.message}`);
  process.exit(1);
});

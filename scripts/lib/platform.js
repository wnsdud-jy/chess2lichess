export const SUPPORTED_TARGETS = Object.freeze([
  'linux-x64',
  'linux-arm64',
  'darwin-x64',
  'darwin-arm64',
  'win32-x64',
]);

const TARGET_META = Object.freeze({
  'linux-x64': { platform: 'linux', arch: 'x64', exeSuffix: '' },
  'linux-arm64': { platform: 'linux', arch: 'arm64', exeSuffix: '' },
  'darwin-x64': { platform: 'darwin', arch: 'x64', exeSuffix: '' },
  'darwin-arm64': { platform: 'darwin', arch: 'arm64', exeSuffix: '' },
  'win32-x64': { platform: 'win32', arch: 'x64', exeSuffix: '.exe' },
});

export function normalizeVersion(version) {
  if (typeof version !== 'string' || version.trim() === '') {
    throw new Error('Version string is required.');
  }
  return version.replace(/^v/, '');
}

export function resolveTarget(platform = process.platform, arch = process.arch) {
  const target = `${platform}-${arch}`;
  if (!SUPPORTED_TARGETS.includes(target)) {
    const supported = SUPPORTED_TARGETS.join(', ');
    throw new Error(
      `Unsupported platform/arch: ${platform}/${arch}. Supported targets: ${supported}`
    );
  }
  return target;
}

export function getTargetMeta(target) {
  const meta = TARGET_META[target];
  if (!meta) {
    throw new Error(`Unknown target: ${target}`);
  }
  return meta;
}

export function buildAssetName(version, target) {
  const normalizedVersion = normalizeVersion(version);
  const meta = getTargetMeta(target);
  return `c2l-v${normalizedVersion}-${target}${meta.exeSuffix}`;
}

export function buildChecksumsName(version) {
  const normalizedVersion = normalizeVersion(version);
  return `c2l-v${normalizedVersion}-checksums.txt`;
}

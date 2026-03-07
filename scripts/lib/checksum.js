const CHECKSUM_LINE = /^([A-Fa-f0-9]{64})\s+\*?(.+)$/;

export function parseChecksums(raw) {
  const map = new Map();

  for (const line of raw.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }

    const match = trimmed.match(CHECKSUM_LINE);
    if (!match) {
      continue;
    }

    map.set(match[2], match[1].toLowerCase());
  }

  return map;
}

export function matchChecksum(checksums, filename) {
  const expected = checksums.get(filename);
  if (!expected) {
    throw new Error(`Missing checksum entry for ${filename}`);
  }
  return expected;
}

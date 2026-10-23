export const supportedWindowsRuntimes = Object.freeze({
  "win-x64": { directory: "windows-x64", machine: 0x8664 },
  "win-arm64": { directory: "windows-arm64", machine: 0xaa64 },
});

export function windowsPlatformFromPortableExecutable(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 0x40) {
    throw new Error("Windows worker is not a bounded PE executable.");
  }
  if (bytes[0] !== 0x4d || bytes[1] !== 0x5a) {
    throw new Error("Windows worker is missing the DOS executable header.");
  }
  const peOffset = bytes.readUInt32LE(0x3c);
  if (peOffset > bytes.length - 6) {
    throw new Error("Windows worker has an invalid PE header offset.");
  }
  if (
    bytes[peOffset] !== 0x50 ||
    bytes[peOffset + 1] !== 0x45 ||
    bytes[peOffset + 2] !== 0 ||
    bytes[peOffset + 3] !== 0
  ) {
    throw new Error("Windows worker is missing the PE signature.");
  }
  const machine = bytes.readUInt16LE(peOffset + 4);
  for (const { directory, machine: expected } of Object.values(
    supportedWindowsRuntimes,
  )) {
    if (machine === expected) return directory;
  }
  throw new Error(`Unsupported Windows PE machine: 0x${machine.toString(16)}.`);
}

export function readDeclaredVersion(source, label) {
  for (const pattern of [
    /<Version>\s*(\d+\.\d+\.\d+)\s*<\/Version>/,
    /\bversion\s*[:=]\s*"(\d+\.\d+\.\d+)"/i,
  ]) {
    const match = source.match(pattern);
    if (match) return match[1];
  }
  throw new Error(`${label} does not declare a semantic version.`);
}

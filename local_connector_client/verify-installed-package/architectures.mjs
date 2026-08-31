// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import fs from 'node:fs';
import path from 'node:path';

function parseMachOArchitectures(buffer) {
  if (buffer.length < 8) {
    return [];
  }
  const architectures = [];
  const littleMagic = buffer.readUInt32LE(0);
  if (littleMagic === 0xfeedface || littleMagic === 0xfeedfacf) {
    const cpuType = buffer.readUInt32LE(4);
    if (cpuType === 0x01000007) architectures.push('x64');
    if (cpuType === 0x0100000c) architectures.push('arm64');
    return architectures;
  }
  const bigMagic = buffer.readUInt32BE(0);
  if (bigMagic === 0xfeedface || bigMagic === 0xfeedfacf) {
    const cpuType = buffer.readUInt32BE(4);
    if (cpuType === 0x01000007) architectures.push('x64');
    if (cpuType === 0x0100000c) architectures.push('arm64');
    return architectures;
  }
  if (![0xcafebabe, 0xcafebabf].includes(bigMagic)) {
    return architectures;
  }
  const count = buffer.readUInt32BE(4);
  const stride = bigMagic === 0xcafebabf ? 32 : 20;
  if (count === 0 || count > 16 || buffer.length < 8 + (count * stride)) {
    return architectures;
  }
  for (let index = 0; index < count; index += 1) {
    const cpuType = buffer.readUInt32BE(8 + (index * stride));
    if (cpuType === 0x01000007) architectures.push('x64');
    if (cpuType === 0x0100000c) architectures.push('arm64');
  }
  return [...new Set(architectures)];
}

function parsePeArchitecture(buffer) {
  if (buffer.length < 0x40 || buffer[0] !== 0x4d || buffer[1] !== 0x5a) {
    return [];
  }
  const peOffset = buffer.readUInt32LE(0x3c);
  if (peOffset < 0 || peOffset + 6 > buffer.length || buffer.toString('ascii', peOffset, peOffset + 4) !== 'PE\0\0') {
    return [];
  }
  const machine = buffer.readUInt16LE(peOffset + 4);
  if (machine === 0x8664) return ['x64'];
  if (machine === 0xaa64) return ['arm64'];
  return [];
}

function parseElfArchitecture(buffer) {
  if (buffer.length < 20
    || buffer[0] !== 0x7f
    || buffer[1] !== 0x45
    || buffer[2] !== 0x4c
    || buffer[3] !== 0x46) {
    return [];
  }
  const endianness = buffer[5];
  if (![1, 2].includes(endianness)) {
    return [];
  }
  const machine = endianness === 1 ? buffer.readUInt16LE(18) : buffer.readUInt16BE(18);
  if (machine === 0x3e) return ['x64'];
  if (machine === 0xb7) return ['arm64'];
  return [];
}

export function binaryArchitectures(filePath, platform) {
  const fd = fs.openSync(filePath, 'r');
  try {
    const buffer = Buffer.alloc(4096);
    const bytesRead = fs.readSync(fd, buffer, 0, buffer.length, 0);
    const header = buffer.subarray(0, bytesRead);
    if (platform.startsWith('macos-')) return parseMachOArchitectures(header);
    if (platform.startsWith('windows-')) return parsePeArchitecture(header);
    return parseElfArchitecture(header);
  } finally {
    fs.closeSync(fd);
  }
}

export function assertNoObsoleteCriticalAliases(resources, platform) {
  const aliases = [
    'chrome_extension',
    'chatos_frontend',
    'sqlite_migrations',
  ];
  for (const alias of aliases) {
    if (fs.existsSync(path.join(resources, alias))) {
      throw new Error(`Installed package contains an obsolete critical resource alias: ${alias}`);
    }
  }
  const unexpectedBinaries = platform.startsWith('macos-')
    ? ['local_connector_client_core.exe']
    : platform.startsWith('windows-')
      ? ['local_connector_client_core']
      : ['local_connector_client_core.exe'];
  for (const fileName of unexpectedBinaries) {
    if (fs.existsSync(path.join(resources, fileName))) {
      throw new Error(`Installed package contains an unexpected critical executable: ${fileName}`);
    }
  }
}

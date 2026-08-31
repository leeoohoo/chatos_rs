// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import test, { after, before } from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const CLIENT_DIR = path.dirname(fileURLToPath(import.meta.url));
const VERIFIER = path.join(CLIENT_DIR, 'verify-installed-package.mjs');
const ELECTRON_RUNTIME = path.join(CLIENT_DIR, 'frontend', 'electron', 'core-runtime.cjs');

let temporaryRoot;
let nativeMigrationInventoryFixture;
const baseFixtures = new Map();

function currentPackagePlatform() {
  const platform = {
    darwin: 'macos',
    win32: 'windows',
    linux: 'linux',
  }[process.platform];
  return platform ? `${platform}-${process.arch}` : null;
}

function machOHeader(architecture) {
  const buffer = Buffer.alloc(64);
  buffer.writeUInt32LE(0xfeedfacf, 0);
  buffer.writeUInt32LE(architecture === 'arm64' ? 0x0100000c : 0x01000007, 4);
  return buffer;
}

function peHeader(architecture) {
  const buffer = Buffer.alloc(512);
  buffer.write('MZ', 0, 'ascii');
  buffer.writeUInt32LE(0x80, 0x3c);
  buffer.write('PE\0\0', 0x80, 'ascii');
  buffer.writeUInt16LE(architecture === 'arm64' ? 0xaa64 : 0x8664, 0x84);
  return buffer;
}

function elfHeader(architecture) {
  const buffer = Buffer.alloc(64);
  buffer[0] = 0x7f;
  buffer.write('ELF', 1, 'ascii');
  buffer[4] = 2;
  buffer[5] = 1;
  buffer.writeUInt16LE(architecture === 'arm64' ? 0xb7 : 0x3e, 18);
  return buffer;
}

function binaryHeader(platform, architecture) {
  if (platform.startsWith('macos-')) return machOHeader(architecture);
  if (platform.startsWith('windows-')) return peHeader(architecture);
  return elfHeader(architecture);
}

function writeExecutable(filePath, contents) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents, { mode: 0o755 });
  fs.chmodSync(filePath, 0o755);
}

function compileNativeMigrationInventoryFixture() {
  const platform = currentPackagePlatform();
  if (!platform) return null;
  const versions = fs.readdirSync(path.join(CLIENT_DIR, 'core', 'migrations'))
    .filter((entry) => entry.endsWith('.sql'))
    .map((entry) => entry.match(/^0*(\d+)_.*\.sql$/))
    .filter(Boolean)
    .map((match) => Number.parseInt(match[1], 10))
    .sort((a, b) => a - b)
    .join('\n');
  const source = path.join(temporaryRoot, 'migration-inventory.rs');
  const executable = path.join(
    temporaryRoot,
    process.platform === 'win32' ? 'migration-inventory.exe' : 'migration-inventory',
  );
  fs.writeFileSync(source, `
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--local-runtime-migration-versions") {
        print!(${JSON.stringify(`${versions}\n`)});
        return;
    }
    std::process::exit(2);
}
`);
  const result = spawnSync('rustc', [source, '-o', executable], {
    encoding: 'utf8',
    maxBuffer: 4 * 1024 * 1024,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return { executable, platform };
}

function createFixture(platform, runtimeProfile = 'full') {
  const fixtureRoot = path.join(temporaryRoot, `base-${platform}-${runtimeProfile}`);
  const resources = path.join(fixtureRoot, 'resources');
  fs.mkdirSync(resources, { recursive: true });
  fs.cpSync(
    path.join(CLIENT_DIR, 'core', 'migrations'),
    path.join(resources, 'sqlite-migrations'),
    { recursive: true },
  );
  fs.mkdirSync(path.join(resources, 'app', 'electron'), { recursive: true });
  fs.copyFileSync(ELECTRON_RUNTIME, path.join(resources, 'app', 'electron', 'core-runtime.cjs'));

  const architecture = platform.endsWith('-arm64') ? 'arm64' : 'x64';
  const executableName = platform.startsWith('windows-')
    ? 'local_connector_client_core.exe'
    : 'local_connector_client_core';
  const executablePath = path.join(resources, executableName);
  if (nativeMigrationInventoryFixture?.platform === platform) {
    fs.copyFileSync(nativeMigrationInventoryFixture.executable, executablePath);
    fs.chmodSync(executablePath, 0o755);
  } else {
    writeExecutable(executablePath, binaryHeader(platform, architecture));
  }
  const platformRoot = path.join(resources, 'bundled-tools', platform);
  fs.mkdirSync(platformRoot, { recursive: true });
  fs.writeFileSync(path.join(platformRoot, 'rg'), 'ripgrep-test\n', { mode: 0o755 });
  return resources;
}

function copyFixture(key, name) {
  const root = path.join(temporaryRoot, name);
  fs.cpSync(baseFixtures.get(key), root, { recursive: true });
  return root;
}

function verify(resources, platform, runtimeProfile = 'full') {
  const report = path.join(path.dirname(resources), `${path.basename(resources)}.verification.json`);
  const result = spawnSync(process.execPath, [
    VERIFIER,
    '--platform', platform,
    '--runtime-profile', runtimeProfile,
    '--resources', resources,
    '--electron-runtime-source', ELECTRON_RUNTIME,
    '--report', report,
  ], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  return { ...result, report };
}

before(() => {
  temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-installed-package-test-'));
  nativeMigrationInventoryFixture = compileNativeMigrationInventoryFixture();
  baseFixtures.set('macos-arm64', createFixture('macos-arm64'));
  baseFixtures.set('windows-x64', createFixture('windows-x64'));
  baseFixtures.set('linux-arm64-core', createFixture('linux-arm64', 'linux-core'));
});

after(() => {
  if (temporaryRoot) fs.rmSync(temporaryRoot, { recursive: true, force: true });
});

test('verifies a macOS core-only installed package contract', () => {
  const resources = copyFixture('macos-arm64', 'macos-positive');
  const result = verify(resources, 'macos-arm64');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.equal(report.result, 'verified');
  assert.equal(report.platform, 'macos-arm64');
});

test('verifies a Windows core-only installed package contract', () => {
  const resources = copyFixture('windows-x64', 'windows-positive');
  const result = verify(resources, 'windows-x64');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.deepEqual(report.executables[0].architectures, ['x64']);
});

test('verifies a Linux core-profile installed package contract', () => {
  const resources = copyFixture('linux-arm64-core', 'linux-positive');
  const result = verify(resources, 'linux-arm64', 'linux-core');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.equal(report.runtime_profile, 'linux-core');
});

test('rejects bundled browser capabilities', () => {
  const resources = copyFixture('macos-arm64', 'browser-negative');
  fs.mkdirSync(path.join(resources, 'chrome-extension'), { recursive: true });
  const result = verify(resources, 'macos-arm64');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /removed built-in capability/);
});

test('rejects a symlink substituted for the core executable', () => {
  const resources = copyFixture('macos-arm64', 'symlink-negative');
  const core = path.join(resources, 'local_connector_client_core');
  fs.unlinkSync(core);
  fs.symlinkSync('bundled-tools/macos-arm64/rg', core);
  const result = verify(resources, 'macos-arm64');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must not contain symlink components/);
});

test('packagers verify final resources without browser bundles', () => {
  const macosScript = fs.readFileSync(path.join(CLIENT_DIR, 'package-electron-macos-client.sh'), 'utf8');
  const windowsScript = fs.readFileSync(path.join(CLIENT_DIR, 'package-electron-windows-client.ps1'), 'utf8');
  const linuxScript = fs.readFileSync(path.join(CLIENT_DIR, 'package-electron-linux-client.sh'), 'utf8');
  const macosBuilder = fs.readFileSync(path.join(CLIENT_DIR, 'electron-builder-macos.yml'), 'utf8');
  const linuxBuilder = fs.readFileSync(path.join(CLIENT_DIR, 'electron-builder-linux.yml'), 'utf8');
  assert.ok(macosScript.indexOf('node "$INSTALLED_PACKAGE_VERIFIER"') < macosScript.indexOf('hdiutil verify "$DMG_PATH"'));
  assert.ok(windowsScript.indexOf('Invoke-InstalledPackageVerification -ResourcesDir') < windowsScript.indexOf('Compress-Archive -LiteralPath'));
  assert.ok(linuxScript.indexOf('node "$INSTALLED_PACKAGE_VERIFIER"') < linuxScript.indexOf('[OK] Linux desktop installer'));
  assert.match(linuxScript, /--runtime-profile linux-core/);
  for (const source of [macosScript, windowsScript, linuxScript, macosBuilder, linuxBuilder]) {
    assert.doesNotMatch(source, /chatos_chrome_native_host|chrome-extension|agent-browser/);
  }
});

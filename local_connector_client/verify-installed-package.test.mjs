// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import test, { after, before } from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const CLIENT_DIR = path.dirname(fileURLToPath(import.meta.url));
const VERIFIER = path.join(CLIENT_DIR, 'verify-installed-package.mjs');
const PLUGIN_BUNDLE_TOOL = path.join(CLIENT_DIR, 'prepare-plugin-bundles.mjs');
const PLUGIN_CATALOG = path.join(CLIENT_DIR, 'plugin_bundles', 'catalog', 'bundled-plugin-catalog.json');
const SKILL_CATALOG = path.join(CLIENT_DIR, 'skill_bundles', 'catalog', 'internal-skill-catalog.json');
const SKILL_ROOT = path.join(CLIENT_DIR, 'skill_bundles', 'internal');
const ELECTRON_RUNTIME = path.join(CLIENT_DIR, 'frontend', 'electron', 'core-runtime.cjs');
const CHROME_EXTENSION = path.join(CLIENT_DIR, 'chrome_extension');

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

function compileNativeMigrationInventoryFixture() {
  const platform = currentPackagePlatform();
  if (!platform) return null;
  if (process.env.CHATOS_TEST_LOCAL_CONNECTOR_BINARY) {
    const executable = path.resolve(process.env.CHATOS_TEST_LOCAL_CONNECTOR_BINARY);
    assert.equal(fs.statSync(executable).isFile(), true);
    return { executable, platform };
  }
  const migrationOutput = fs.readdirSync(path.join(CLIENT_DIR, 'core', 'migrations'))
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
        print!(${JSON.stringify(`${migrationOutput}\n`)});
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

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function writeExecutable(filePath, contents) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents, { mode: 0o755 });
  fs.chmodSync(filePath, 0o755);
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

function copyActiveSkillBundles(resources) {
  const catalog = JSON.parse(fs.readFileSync(SKILL_CATALOG, 'utf8'));
  const packagedCatalog = path.join(resources, 'skill-bundles', 'catalog', 'internal-skill-catalog.json');
  fs.mkdirSync(path.dirname(packagedCatalog), { recursive: true });
  fs.copyFileSync(SKILL_CATALOG, packagedCatalog);
  for (const skill of catalog.skills) {
    const source = path.join(SKILL_ROOT, skill.name, skill.version);
    const destination = path.join(resources, 'skill-bundles', 'internal', skill.name, skill.version);
    fs.cpSync(source, destination, { recursive: true });
  }
}

function stagePluginBundles(resources, platform) {
  const result = spawnSync(process.execPath, [
    PLUGIN_BUNDLE_TOOL,
    '--plugin-catalog', PLUGIN_CATALOG,
    '--skill-catalog', path.join(resources, 'skill-bundles', 'catalog', 'internal-skill-catalog.json'),
    '--skill-root', path.join(resources, 'skill-bundles', 'internal'),
    '--output', path.join(resources, 'plugin-bundles'),
    '--platform', platform,
  ], { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 });
  assert.equal(result.status, 0, result.stderr || result.stdout);
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
  if (runtimeProfile === 'full' || runtimeProfile === 'linux-browser') {
    fs.cpSync(CHROME_EXTENSION, path.join(resources, 'chrome-extension'), { recursive: true });
  }
  copyActiveSkillBundles(resources);
  stagePluginBundles(resources, platform);

  const packageArchitecture = platform.endsWith('-arm64') ? 'arm64' : 'x64';
  const executableNames = platform.startsWith('macos-')
    ? ['local_connector_client_core', 'chatos_chrome_native_host', 'chatos_computer_use_helper']
    : platform.startsWith('windows-')
      ? ['local_connector_client_core.exe', 'chatos_chrome_native_host.exe']
      : [
        'local_connector_client_core',
        ...(runtimeProfile === 'linux-browser' ? ['chatos_chrome_native_host'] : []),
      ];
  for (const executableName of executableNames) {
    const destination = path.join(resources, executableName);
    if (
      executableName === (platform.startsWith('windows-') ? 'local_connector_client_core.exe' : 'local_connector_client_core')
      && nativeMigrationInventoryFixture?.platform === platform
    ) {
      fs.copyFileSync(nativeMigrationInventoryFixture.executable, destination);
      fs.chmodSync(destination, 0o755);
    } else {
      writeExecutable(destination, binaryHeader(platform, packageArchitecture));
    }
  }

  const platformRoot = path.join(resources, 'bundled-tools', platform);
  fs.mkdirSync(platformRoot, { recursive: true });
  if (runtimeProfile !== 'full') {
    fs.writeFileSync(path.join(platformRoot, 'rg'), 'ripgrep-test\n', { mode: 0o755 });
    return resources;
  }
  const browserArchitecture = platform.startsWith('windows-') ? 'x64' : packageArchitecture;
  const agentBrowser = path.join(platformRoot, platform.startsWith('windows-') ? 'agent-browser.exe' : 'agent-browser');
  const chrome = platform.startsWith('macos-')
    ? path.join(platformRoot, 'browser', 'Google Chrome for Testing.app', 'Contents', 'MacOS', 'Google Chrome for Testing')
    : path.join(platformRoot, 'browser', 'chrome-win64', 'chrome.exe');
  writeExecutable(agentBrowser, binaryHeader(platform, browserArchitecture));
  writeExecutable(chrome, binaryHeader(platform, browserArchitecture));
  fs.writeFileSync(path.join(platformRoot, 'agent-browser.LICENSE'), 'test license\n');

  const documentRoot = path.join(platformRoot, 'documents-runtime');
  const sofficeRelative = platform.startsWith('macos-')
    ? 'libreoffice/LibreOffice.app/Contents/MacOS/soffice'
    : 'libreoffice/program/soffice.exe';
  const pdftoppmRelative = platform.startsWith('macos-')
    ? 'poppler/bin/pdftoppm'
    : 'poppler/Library/bin/pdftoppm.exe';
  const fontRelative = 'fonts/NotoSansSC-Regular.ttf';
  writeExecutable(path.join(documentRoot, ...sofficeRelative.split('/')), 'soffice-test\n');
  writeExecutable(path.join(documentRoot, ...pdftoppmRelative.split('/')), 'pdftoppm-test\n');
  fs.mkdirSync(path.join(documentRoot, 'fonts'), { recursive: true });
  fs.writeFileSync(path.join(documentRoot, fontRelative), 'font-test\n');
  fs.writeFileSync(path.join(documentRoot, 'fonts', 'NotoSansSC-OFL.txt'), 'font license\n');
  if (platform.startsWith('macos-')) {
    fs.mkdirSync(path.join(documentRoot, 'poppler', 'lib'), { recursive: true });
  }
  const manifest = {
    schema_version: 1,
    runtime_revision: 'test-runtime-1',
    platform,
    soffice: {
      path: sofficeRelative,
      sha256: sha256File(path.join(documentRoot, ...sofficeRelative.split('/'))),
      version: 'LibreOffice test',
    },
    pdftoppm: {
      path: pdftoppmRelative,
      sha256: sha256File(path.join(documentRoot, ...pdftoppmRelative.split('/'))),
      version: 'pdftoppm version test',
    },
    poppler_library_dir: platform.startsWith('macos-') ? 'poppler/lib' : null,
    font_directory: 'fonts',
    fonts: [{ path: fontRelative, sha256: sha256File(path.join(documentRoot, fontRelative)) }],
  };
  fs.writeFileSync(path.join(documentRoot, 'runtime.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  return resources;
}

function copyFixture(platform, name) {
  const root = path.join(temporaryRoot, name);
  fs.cpSync(baseFixtures.get(platform), root, { recursive: true });
  return root;
}

function verify(resources, platform, runtimeProfile = 'full') {
  const report = path.join(path.dirname(resources), `${path.basename(resources)}.verification.json`);
  const result = spawnSync(process.execPath, [
    VERIFIER,
    '--platform', platform,
    '--runtime-profile', runtimeProfile,
    '--resources', resources,
    '--plugin-catalog', PLUGIN_CATALOG,
    '--skill-catalog', SKILL_CATALOG,
    '--electron-runtime-source', ELECTRON_RUNTIME,
    '--chrome-extension-source', CHROME_EXTENSION,
    '--report', report,
  ], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  return { ...result, report };
}

before(() => {
  temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-installed-package-test-'));
  nativeMigrationInventoryFixture = compileNativeMigrationInventoryFixture();
  baseFixtures.set('macos-arm64', createFixture('macos-arm64'));
  baseFixtures.set('windows-x64', createFixture('windows-x64'));
  baseFixtures.set('windows-arm64', createFixture('windows-arm64'));
  baseFixtures.set('linux-arm64-core', createFixture('linux-arm64', 'linux-core'));
  baseFixtures.set('linux-arm64-browser', createFixture('linux-arm64', 'linux-browser'));
});

after(() => {
  if (temporaryRoot) fs.rmSync(temporaryRoot, { recursive: true, force: true });
});

test('verifies a complete macOS arm64 installed package contract', () => {
  const resources = copyFixture('macos-arm64', 'macos-positive');
  const result = verify(resources, 'macos-arm64');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.equal(report.result, 'verified');
  assert.equal(report.platform, 'macos-arm64');
  assert.equal(report.plugin_bundles.plugins, 12);
  assert.equal(report.plugin_bundles.skills, 28);
  assert.equal(report.code_signing.required, false);
  assert.equal(fs.readFileSync(result.report, 'utf8').includes(temporaryRoot), false);
});

test('verifies a complete Windows x64 installed package contract', () => {
  const resources = copyFixture('windows-x64', 'windows-positive');
  const result = verify(resources, 'windows-x64');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.equal(report.result, 'verified');
  assert.equal(report.platform, 'windows-x64');
  assert.equal(report.browser_runtime.windows_arm64_x64_emulation, false);
});

test('requires ARM64 Windows hosts while allowing the pinned x64 browser runtime', () => {
  const resources = copyFixture('windows-arm64', 'windows-arm64-positive');
  const result = verify(resources, 'windows-arm64');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.deepEqual(report.executables[0].architectures, ['arm64']);
  assert.deepEqual(report.browser_runtime.agent_browser.architectures, ['x64']);
  assert.equal(report.browser_runtime.windows_arm64_x64_emulation, true);
});

test('verifies a Linux arm64 core-profile installed package contract', () => {
  const resources = copyFixture('linux-arm64-core', 'linux-arm64-core-positive');
  const result = verify(resources, 'linux-arm64', 'linux-core');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.equal(report.platform, 'linux-arm64');
  assert.equal(report.runtime_profile, 'linux-core');
  assert.deepEqual(report.executables[0].architectures, ['arm64']);
  assert.equal(report.browser_runtime.verified, false);
  assert.equal(report.document_runtime.verified, false);
});

test('verifies a Linux arm64 browser-profile package with Native Messaging assets', () => {
  const resources = copyFixture('linux-arm64-browser', 'linux-arm64-browser-positive');
  const result = verify(resources, 'linux-arm64', 'linux-browser');
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(fs.readFileSync(result.report, 'utf8'));
  assert.equal(report.platform, 'linux-arm64');
  assert.equal(report.runtime_profile, 'linux-browser');
  assert.equal(report.chrome_extension.manifest_version, 3);
  assert.ok(report.executables.some((executable) => executable.name === 'chatos_chrome_native_host'));
  assert.equal(report.browser_runtime.verified, false);
  assert.equal(report.document_runtime.verified, false);
});

test('rejects a symlink substituted for a critical executable', () => {
  const resources = copyFixture('macos-arm64', 'symlink-negative');
  const core = path.join(resources, 'local_connector_client_core');
  fs.unlinkSync(core);
  fs.symlinkSync('bundled-tools/macos-arm64/agent-browser', core);
  const result = verify(resources, 'macos-arm64');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must not contain symlink components/);
});

test('rejects a document runtime file whose manifest hash no longer matches', () => {
  const resources = copyFixture('macos-arm64', 'document-hash-negative');
  fs.appendFileSync(
    path.join(resources, 'bundled-tools', 'macos-arm64', 'documents-runtime', 'fonts', 'NotoSansSC-Regular.ttf'),
    'tampered',
  );
  const result = verify(resources, 'macos-arm64');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /fallback font SHA-256 does not match/);
});

test('rejects a critical executable built for the wrong architecture', () => {
  const resources = copyFixture('macos-arm64', 'architecture-negative');
  writeExecutable(path.join(resources, 'chatos_chrome_native_host'), machOHeader('x64'));
  const result = verify(resources, 'macos-arm64');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /architecture mismatch/);
});

test('runs final-resource verification before accepting macOS, Windows, and Linux archives', () => {
  const macosScript = fs.readFileSync(path.join(CLIENT_DIR, 'package-electron-macos-client.sh'), 'utf8');
  const windowsScript = fs.readFileSync(path.join(CLIENT_DIR, 'package-electron-windows-client.ps1'), 'utf8');
  const linuxScript = fs.readFileSync(path.join(CLIENT_DIR, 'package-electron-linux-client.sh'), 'utf8');
  const macosBuilder = fs.readFileSync(path.join(CLIENT_DIR, 'electron-builder-macos.yml'), 'utf8');
  const linuxBuilder = fs.readFileSync(path.join(CLIENT_DIR, 'electron-builder-linux.yml'), 'utf8');
  assert.ok(macosScript.indexOf('node "$INSTALLED_PACKAGE_VERIFIER"') < macosScript.indexOf('hdiutil verify "$DMG_PATH"'));
  assert.ok(windowsScript.indexOf('Invoke-InstalledPackageVerification -ResourcesDir') < windowsScript.indexOf('Compress-Archive -LiteralPath'));
  assert.ok(linuxScript.indexOf('node "$INSTALLED_PACKAGE_VERIFIER"') < linuxScript.indexOf('[OK] Linux desktop installer'));
  assert.match(macosScript, /VERIFY_ARGS\+=\(--require-signed\)/);
  assert.match(windowsScript, /\$verificationReport = "\$zipPath\.verification\.json"/);
  assert.match(linuxScript, /--runtime-profile linux-browser/);
  assert.match(linuxBuilder, /from: \.\.\/\.package\/linux\/chatos_chrome_native_host/);
  assert.match(linuxBuilder, /from: \.\.\/\.package\/linux\/chrome-extension/);
  assert.match(macosBuilder, /asarUnpack:\n  - electron\/core-runtime\.cjs/);
  assert.match(linuxBuilder, /asarUnpack:\n  - electron\/core-runtime\.cjs/);
});

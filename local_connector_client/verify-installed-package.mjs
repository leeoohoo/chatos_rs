#!/usr/bin/env node
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { assertNoObsoleteCriticalAliases, binaryArchitectures } from './verify-installed-package/architectures.mjs';
import { verifyMacCodeSigning } from './verify-installed-package/codesign.mjs';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const SAFE_PLATFORM = /^(?:macos|windows|linux)-(?:arm64|x64)$/;
const SAFE_RUNTIME_PROFILES = new Set(['full', 'linux-core']);
const SAFE_SHA256 = /^[0-9a-f]{64}$/;
const MAX_RESOURCE_FILES = 300_000;
const MAX_RESOURCE_BYTES = 8 * 1024 * 1024 * 1024;
const MAX_JSON_BYTES = 4 * 1024 * 1024;

function parseArgs(argv) {
  const args = { requireSigned: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--require-signed') {
      args.requireSigned = true;
      continue;
    }
    if (!argument.startsWith('--') || index + 1 >= argv.length) {
      throw new Error(`Invalid argument: ${argument}`);
    }
    const key = argument.slice(2).replace(/-([a-z])/g, (_, char) => char.toUpperCase());
    args[key] = argv[++index];
  }
  for (const required of ['platform', 'resources']) {
    if (!args[required]) {
      const name = required.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`);
      throw new Error(`Missing required --${name}`);
    }
  }
  if (!SAFE_PLATFORM.test(args.platform)) {
    throw new Error(`Unsupported installed package platform: ${args.platform}`);
  }
  args.runtimeProfile = args.runtimeProfile || 'full';
  if (!SAFE_RUNTIME_PROFILES.has(args.runtimeProfile)) {
    throw new Error(`Unsupported installed package runtime profile: ${args.runtimeProfile}`);
  }
  if (args.runtimeProfile.startsWith('linux-') && !args.platform.startsWith('linux-')) {
    throw new Error(`${args.runtimeProfile} runtime profile is only valid for Linux packages`);
  }
  args.resources = path.resolve(args.resources);
  args.electronRuntimeSource = path.resolve(
    args.electronRuntimeSource || path.join(SCRIPT_DIR, 'frontend', 'electron', 'core-runtime.cjs'),
  );
  if (args.report) {
    args.report = path.resolve(args.report);
  }
  return args;
}
function assertRootDirectory(root, label) {
  let stat;
  try {
    stat = fs.lstatSync(root);
  } catch {
    throw new Error(`${label} is missing`);
  }
  if (stat.isSymbolicLink() || !stat.isDirectory()) {
    throw new Error(`${label} must be a regular non-symlink directory`);
  }
}

function normalizeRelativePath(value, label) {
  if (typeof value !== 'string' || value.length === 0 || value.includes('\0') || value.includes('\\')) {
    throw new Error(`${label} must be a non-empty portable relative path`);
  }
  if (path.posix.isAbsolute(value)) {
    throw new Error(`${label} must not be absolute`);
  }
  const parts = value.split('/');
  if (parts.some((part) => !part || part === '.' || part === '..')) {
    throw new Error(`${label} contains an unsafe path segment`);
  }
  const normalized = path.posix.normalize(value);
  if (normalized !== value || normalized.startsWith('../')) {
    throw new Error(`${label} is not canonical`);
  }
  return value;
}

function resolveInside(root, relativePath, label) {
  const normalized = normalizeRelativePath(relativePath, label);
  const absolute = path.resolve(root, ...normalized.split('/'));
  const prefix = `${path.resolve(root)}${path.sep}`;
  if (!absolute.startsWith(prefix)) {
    throw new Error(`${label} escapes its resource root`);
  }
  return absolute;
}

function assertNoSymlinkComponents(root, relativePath, label) {
  const normalized = normalizeRelativePath(relativePath, label);
  let current = root;
  for (const part of normalized.split('/')) {
    current = path.join(current, part);
    let stat;
    try {
      stat = fs.lstatSync(current);
    } catch {
      throw new Error(`${label} is missing`);
    }
    if (stat.isSymbolicLink()) {
      throw new Error(`${label} must not contain symlink components`);
    }
  }
  return current;
}

function requireRegularFile(root, relativePath, label, { executable = false } = {}) {
  const absolute = assertNoSymlinkComponents(root, relativePath, label);
  const stat = fs.lstatSync(absolute);
  if (!stat.isFile()) {
    throw new Error(`${label} must be a regular file`);
  }
  if (stat.size <= 0) {
    throw new Error(`${label} must not be empty`);
  }
  if (executable && process.platform !== 'win32' && (stat.mode & 0o111) === 0) {
    throw new Error(`${label} must have an executable mode bit`);
  }
  return { absolute, size: stat.size };
}

function requireDirectory(root, relativePath, label) {
  const absolute = assertNoSymlinkComponents(root, relativePath, label);
  if (!fs.lstatSync(absolute).isDirectory()) {
    throw new Error(`${label} must be a directory`);
  }
  return absolute;
}

function readJson(filePath, label) {
  const stat = fs.lstatSync(filePath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > MAX_JSON_BYTES) {
    throw new Error(`${label} must be a regular JSON file no larger than 4 MiB`);
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    throw new Error(`${label} is invalid JSON: ${error.message}`);
  }
}

async function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  await new Promise((resolve, reject) => {
    const input = fs.createReadStream(filePath);
    input.on('data', (chunk) => hash.update(chunk));
    input.on('error', reject);
    input.on('end', resolve);
  });
  return hash.digest('hex');
}

function sha256Text(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

function stableJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map(stableJson).join(',')}]`;
  }
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function isInside(root, candidate) {
  const relative = path.relative(path.resolve(root), path.resolve(candidate));
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}

function scanResourceTree(root) {
  const canonicalRoot = fs.realpathSync(root);
  let fileCount = 0;
  let directoryCount = 1;
  let symlinkCount = 0;
  let totalBytes = 0;
  const foldedPaths = new Map();

  function visit(absoluteRoot, relativeRoot) {
    const entries = fs.readdirSync(absoluteRoot, { withFileTypes: true })
      .sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const relativePath = relativeRoot ? `${relativeRoot}/${entry.name}` : entry.name;
      const folded = relativePath.toLocaleLowerCase('en-US');
      const previous = foldedPaths.get(folded);
      if (previous && previous !== relativePath) {
        throw new Error(`Resource tree contains a case-insensitive path collision: ${previous} / ${relativePath}`);
      }
      foldedPaths.set(folded, relativePath);
      const absolutePath = path.join(absoluteRoot, entry.name);
      const stat = fs.lstatSync(absolutePath);
      if (stat.isSymbolicLink()) {
        symlinkCount += 1;
        let resolved;
        try {
          resolved = fs.realpathSync(absolutePath);
        } catch {
          throw new Error(`Resource tree contains a broken symlink: ${relativePath}`);
        }
        if (!isInside(canonicalRoot, resolved)) {
          throw new Error(`Resource tree symlink escapes the package: ${relativePath}`);
        }
      } else if (stat.isDirectory()) {
        directoryCount += 1;
        visit(absolutePath, relativePath);
      } else if (stat.isFile()) {
        fileCount += 1;
        totalBytes += stat.size;
      } else {
        throw new Error(`Resource tree contains a special file: ${relativePath}`);
      }
      if (fileCount > MAX_RESOURCE_FILES || totalBytes > MAX_RESOURCE_BYTES) {
        throw new Error('Resource tree exceeds the installed-package verification limits');
      }
    }
  }

  visit(root, '');
  return { file_count: fileCount, directory_count: directoryCount, symlink_count: symlinkCount, total_bytes: totalBytes };
}

function listRegularFiles(root, relativeRoot = '') {
  const files = [];
  const absoluteRoot = relativeRoot ? resolveInside(root, relativeRoot, 'tree path') : root;
  for (const entry of fs.readdirSync(absoluteRoot, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const relativePath = relativeRoot ? `${relativeRoot}/${entry.name}` : entry.name;
    const absolutePath = path.join(absoluteRoot, entry.name);
    const stat = fs.lstatSync(absolutePath);
    if (stat.isSymbolicLink() || (!stat.isDirectory() && !stat.isFile())) {
      throw new Error(`Verified resource subtree contains a symlink or special file: ${relativePath}`);
    }
    if (stat.isDirectory()) {
      files.push(...listRegularFiles(root, relativePath));
    } else {
      files.push(relativePath);
    }
  }
  return files;
}

async function compareExactTrees(sourceRoot, packagedRoot, label) {
  assertRootDirectory(sourceRoot, `${label} source`);
  assertRootDirectory(packagedRoot, `${label} package directory`);
  const sourceFiles = listRegularFiles(sourceRoot);
  const packagedFiles = listRegularFiles(packagedRoot);
  if (JSON.stringify(sourceFiles) !== JSON.stringify(packagedFiles)) {
    throw new Error(`${label} packaged file list differs from its release source`);
  }
  for (const relativePath of sourceFiles) {
    const sourceHash = await sha256File(resolveInside(sourceRoot, relativePath, `${label} source file`));
    const packagedHash = await sha256File(resolveInside(packagedRoot, relativePath, `${label} packaged file`));
    if (sourceHash !== packagedHash) {
      throw new Error(`${label} packaged file hash differs from its release source: ${relativePath}`);
    }
  }
  return { file_count: sourceFiles.length };
}


async function verifyBinary(resources, relativePath, label, platform, expectedArchitecture) {
  const file = requireRegularFile(resources, relativePath, label, {
    executable: !platform.startsWith('windows-'),
  });
  const architectures = binaryArchitectures(file.absolute, platform);
  if (!architectures.includes(expectedArchitecture)) {
    throw new Error(`${label} architecture mismatch: expected ${expectedArchitecture}, found ${architectures.join(',') || 'unknown'}`);
  }
  return {
    name: relativePath,
    architectures,
    bytes: file.size,
    sha256: await sha256File(file.absolute),
  };
}


function migrationVersionsFromDirectory(migrationsDir) {
  const versions = fs.readdirSync(migrationsDir)
    .filter((entry) => entry.endsWith('.sql'))
    .map((entry) => {
      const match = entry.match(/^0*(\d+)_.*\.sql$/);
      if (!match) {
        throw new Error(`SQLite migration has an unsupported filename: ${entry}`);
      }
      return Number.parseInt(match[1], 10);
    })
    .sort((a, b) => a - b);
  if (!versions.length) {
    throw new Error('Installed package contains no SQLite migrations');
  }
  if (new Set(versions).size !== versions.length) {
    throw new Error('Installed package contains duplicate SQLite migration versions');
  }
  return versions;
}

function currentProcessCanExecuteTarget(platform) {
  const currentPlatform = { darwin: 'macos', win32: 'windows', linux: 'linux' }[process.platform];
  return platform === `${currentPlatform}-${process.arch}`;
}

function verifyEmbeddedSqliteMigrations(args, executablePath, migrationsDir) {
  const resourceVersions = migrationVersionsFromDirectory(migrationsDir);
  if (!currentProcessCanExecuteTarget(args.platform)) {
    return {
      verified: false,
      reason: 'target executable cannot be run on this host',
      resource_versions: resourceVersions,
    };
  }
  const result = spawnSync(executablePath, ['--local-runtime-migration-versions'], {
    encoding: 'utf8',
    maxBuffer: 1024 * 1024,
    timeout: 10_000,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`Local Connector Core migration inventory failed: ${result.stderr || result.stdout || result.status}`);
  }
  const embeddedVersions = result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => Number.parseInt(line, 10));
  if (
    embeddedVersions.some((version) => !Number.isSafeInteger(version))
    || JSON.stringify(embeddedVersions) !== JSON.stringify(resourceVersions)
  ) {
    throw new Error(
      `Local Connector Core embedded SQLite migrations do not match packaged resources: embedded=${embeddedVersions.join(',')} resources=${resourceVersions.join(',')}`,
    );
  }
  return {
    verified: true,
    resource_versions: resourceVersions,
    embedded_versions: embeddedVersions,
  };
}



function assertNoRemovedBuiltinCapabilities(args) {
  const forbiddenPaths = [
    'skill-bundles',
    'chrome-extension',
    'chatos_chrome_native_host',
    'chatos_chrome_native_host.exe',
    'chatos_computer_use_helper',
    'chatos_computer_use_helper.exe',
    `bundled-tools/${args.platform}/agent-browser`,
    `bundled-tools/${args.platform}/agent-browser.exe`,
    `bundled-tools/${args.platform}/browser`,
    `bundled-tools/${args.platform}/documents-runtime`,
  ];
  for (const relativePath of forbiddenPaths) {
    if (fs.existsSync(path.join(args.resources, ...relativePath.split('/')))) {
      throw new Error(`Installed package contains removed built-in capability: ${relativePath}`);
    }
  }
}

async function verifyElectronRuntime(args) {
  const candidates = [
    'app/electron/core-runtime.cjs',
    'app.asar.unpacked/electron/core-runtime.cjs',
  ].filter((relativePath) => fs.existsSync(path.join(args.resources, ...relativePath.split('/'))));
  if (candidates.length !== 1) {
    throw new Error('Installed Electron runtime must expose exactly one verifiable core-runtime.cjs copy');
  }
  const packaged = requireRegularFile(args.resources, candidates[0], 'Packaged Electron core runtime').absolute;
  const source = requireRegularFile(path.dirname(args.electronRuntimeSource), path.basename(args.electronRuntimeSource), 'Electron core runtime source').absolute;
  const packagedHash = await sha256File(packaged);
  const sourceHash = await sha256File(source);
  if (packagedHash !== sourceHash) {
    throw new Error('Packaged Electron core runtime differs from its release source');
  }
  const runtime = fs.readFileSync(packaged, 'utf8');
  const requiredPatterns = [
    /process\.resourcesPath/,
    /CHATOS_BUNDLED_TOOLS_DIR:\s*resourcePath\('bundled-tools'\)/,
  ];
  if (requiredPatterns.some((pattern) => !pattern.test(runtime))) {
    throw new Error('Packaged Electron core runtime is missing a required packaged-resource binding');
  }
  return { packaged_path: candidates[0], bytes: fs.lstatSync(packaged).size, sha256: packagedHash };
}




function writeReport(reportPath, report) {
  const parent = path.dirname(reportPath);
  assertRootDirectory(parent, 'Verification report parent');
  if (fs.existsSync(reportPath) && fs.lstatSync(reportPath).isSymbolicLink()) {
    throw new Error('Verification report path must not be a symlink');
  }
  const temporary = `${reportPath}.partial-${process.pid}`;
  try {
    fs.writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o644 });
    fs.rmSync(reportPath, { force: true });
    fs.renameSync(temporary, reportPath);
  } finally {
    fs.rmSync(temporary, { force: true });
  }
}

function sanitizedError(error, args) {
  let message = String(error?.stack || error);
  const replacements = [
    [args?.resources, '<resources>'],
    [args?.electronRuntimeSource, '<electron-runtime-source>'],
    [SCRIPT_DIR, '<local-connector-client>'],
    [process.env.HOME, '<home>'],
  ];
  for (const [value, replacement] of replacements) {
    if (value) message = message.split(value).join(replacement);
  }
  return message;
}

async function main(args) {
  assertRootDirectory(args.resources, 'Installed package resources root');
  assertNoObsoleteCriticalAliases(args.resources, args.platform);
  assertNoRemovedBuiltinCapabilities(args);
  const tree = scanResourceTree(args.resources);
  const migrations = requireDirectory(args.resources, 'sqlite-migrations', 'SQLite migrations');

  const packageArchitecture = args.platform.endsWith('-arm64') ? 'arm64' : 'x64';
  const macos = args.platform.startsWith('macos-');
  const windows = args.platform.startsWith('windows-');
  const binarySpecs = macos
    ? [['local_connector_client_core', 'Local Connector Core']]
    : windows
      ? [['local_connector_client_core.exe', 'Local Connector Core']]
      : [['local_connector_client_core', 'Local Connector Core']];
  const executables = [];
  const executablePaths = [];
  for (const [relativePath, label] of binarySpecs) {
    const verified = await verifyBinary(args.resources, relativePath, label, args.platform, packageArchitecture);
    executables.push(verified);
    executablePaths.push(path.join(args.resources, relativePath));
  }
  const localRuntimeMigrations = verifyEmbeddedSqliteMigrations(args, executablePaths[0], migrations);

  const electronRuntime = await verifyElectronRuntime(args);
  const codeSigning = verifyMacCodeSigning(args, executablePaths);
  const report = {
    schema_version: 1,
    result: 'verified',
    verified_at: new Date().toISOString(),
    platform: args.platform,
    runtime_profile: args.runtimeProfile,
    resource_tree: tree,
    executables,
    local_runtime_migrations: localRuntimeMigrations,
    electron_runtime: electronRuntime,
    code_signing: codeSigning,
  };
  if (args.report) {
    writeReport(args.report, report);
  }
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
}

let parsedArgs;
try {
  parsedArgs = parseArgs(process.argv.slice(2));
  await main(parsedArgs);
} catch (error) {
  process.stderr.write(`[ERROR] ${sanitizedError(error, parsedArgs)}\n`);
  process.exitCode = 1;
}

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
const SAFE_RUNTIME_PROFILES = new Set(['full', 'linux-core', 'linux-browser']);
const SAFE_SHA256 = /^[0-9a-f]{64}$/;
const SAFE_RUNTIME_REVISION = /^[0-9A-Za-z][0-9A-Za-z._-]{0,159}$/;
const MAX_RESOURCE_FILES = 300_000;
const MAX_RESOURCE_BYTES = 8 * 1024 * 1024 * 1024;
const MAX_JSON_BYTES = 4 * 1024 * 1024;
const EXPECTED_SKILL_COUNT = 28;
const EXPECTED_PLUGIN_COUNT = 12;
const EXPECTED_CHROME_EXTENSION_KEY = 'MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAwtyBLDERxm2J31roRxBzHGFmtn03x51KFG7KLXkLNzNVaEnk6Np4ZnQMiu7ADkVykLoDtBUZCcJ5/Ol7Ceo9eYGOdtKp1KPpW5tM16vj+y0NkwOi27Ofr9ak0P3MvHQnJjAFOHd/vOSF8El94VV6A6iWuhlGSbnvbj+oZ+w3RWQkqKiXr/Qkd77DvvJhQghcz0V5JhVqrMANxOW1kPDVPIZvPfrxh4+LX4jrzPSLzgQcsG6q6M4dkdIH7UeymQv12XVdP2UtSrLyTRC2MpzuohQmau334GnZAGfkfg9ODXbrVdlabFb4JnhZHVCEoMwNI0wNhbkTlxG1bhZlgQTQawIDAQAB';
const EXPECTED_CHROME_PERMISSIONS = [
  'activeTab',
  'nativeMessaging',
  'scripting',
  'storage',
];
const FORBIDDEN_CHROME_PERMISSIONS = new Set([
  'bookmarks',
  'browsingData',
  'clipboardRead',
  'clipboardWrite',
  'contentSettings',
  'cookies',
  'debugger',
  'downloads',
  'downloads.open',
  'geolocation',
  'history',
  'management',
  'privacy',
  'proxy',
  'sessions',
  'tabs',
  'webNavigation',
  'webRequest',
  'webRequestBlocking',
]);

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
  for (const required of ['platform', 'resources', 'pluginCatalog', 'skillCatalog']) {
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
  args.pluginCatalog = path.resolve(args.pluginCatalog);
  args.skillCatalog = path.resolve(args.skillCatalog);
  args.electronRuntimeSource = path.resolve(
    args.electronRuntimeSource || path.join(SCRIPT_DIR, 'frontend', 'electron', 'core-runtime.cjs'),
  );
  args.chromeExtensionSource = path.resolve(
    args.chromeExtensionSource || path.join(SCRIPT_DIR, 'chrome_extension'),
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

async function verifyChromeExtension(args) {
  const packagedRoot = requireDirectory(args.resources, 'chrome-extension', 'Chrome extension');
  const sourceComparison = await compareExactTrees(args.chromeExtensionSource, packagedRoot, 'Chrome extension');
  const manifestPath = requireRegularFile(packagedRoot, 'manifest.json', 'Chrome extension manifest').absolute;
  const manifest = readJson(manifestPath, 'Chrome extension manifest');
  if (manifest.manifest_version !== 3 || manifest.key !== EXPECTED_CHROME_EXTENSION_KEY) {
    throw new Error('Chrome extension must use Manifest V3 and the fixed ChatOS public key');
  }
  if (!Array.isArray(manifest.permissions)) {
    throw new Error('Chrome extension permissions must be an array');
  }
  const permissions = [...new Set(manifest.permissions)].sort();
  if (permissions.length !== manifest.permissions.length
    || JSON.stringify(permissions) !== JSON.stringify([...EXPECTED_CHROME_PERMISSIONS].sort())) {
    throw new Error('Chrome extension permissions differ from the least-privilege allowlist');
  }
  for (const permission of permissions) {
    if (FORBIDDEN_CHROME_PERMISSIONS.has(permission)) {
      throw new Error(`Chrome extension contains forbidden permission: ${permission}`);
    }
  }
  const optionalHosts = [...(manifest.optional_host_permissions || [])].sort();
  if (JSON.stringify(optionalHosts) !== JSON.stringify(['http://*/*', 'https://*/*'])) {
    throw new Error('Chrome extension optional host permissions must remain user-granted HTTP/HTTPS only');
  }
  if (manifest.host_permissions || manifest.content_scripts || manifest.externally_connectable) {
    throw new Error('Chrome extension must not declare eager hosts, content scripts, or externally connectable origins');
  }
  for (const referenced of [manifest.background?.service_worker, manifest.action?.default_popup]) {
    if (typeof referenced !== 'string') {
      throw new Error('Chrome extension is missing a required background or popup entrypoint');
    }
    requireRegularFile(packagedRoot, normalizeRelativePath(referenced, 'Chrome extension entrypoint'), 'Chrome extension entrypoint');
  }
  return {
    manifest_version: manifest.manifest_version,
    version: manifest.version,
    fixed_key_sha256: sha256Text(manifest.key),
    permissions,
    optional_host_permissions: optionalHosts,
    files: sourceComparison.file_count,
  };
}

async function verifyBrowserRuntime(args) {
  const platformRoot = `bundled-tools/${args.platform}`;
  requireDirectory(args.resources, platformRoot, 'Bundled tools platform directory');
  const packageArchitecture = args.platform.endsWith('-arm64') ? 'arm64' : 'x64';
  const agentRelative = `${platformRoot}/${args.platform.startsWith('windows-') ? 'agent-browser.exe' : 'agent-browser'}`;
  const chromeRelative = args.platform.startsWith('macos-')
    ? `${platformRoot}/browser/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`
    : `${platformRoot}/browser/chrome-win64/chrome.exe`;
  const browserArchitecture = args.platform.startsWith('windows-') ? 'x64' : packageArchitecture;
  const agent = await verifyBinary(args.resources, agentRelative, 'agent-browser runtime', args.platform, browserArchitecture);
  const chrome = await verifyBinary(args.resources, chromeRelative, 'Chrome for Testing runtime', args.platform, browserArchitecture);
  requireRegularFile(args.resources, `${platformRoot}/agent-browser.LICENSE`, 'agent-browser license');
  return {
    platform: args.platform,
    windows_arm64_x64_emulation: args.platform === 'windows-arm64',
    agent_browser: agent,
    chrome_for_testing: chrome,
  };
}

async function verifyDocumentRuntime(args) {
  const runtimeRelative = `bundled-tools/${args.platform}/documents-runtime`;
  const runtimeRoot = requireDirectory(args.resources, runtimeRelative, 'Document runtime');
  const manifestPath = requireRegularFile(runtimeRoot, 'runtime.json', 'Document runtime manifest').absolute;
  const manifest = readJson(manifestPath, 'Document runtime manifest');
  if (manifest.schema_version !== 1 || manifest.platform !== args.platform) {
    throw new Error('Document runtime manifest schema/platform does not match the package');
  }
  if (typeof manifest.runtime_revision !== 'string' || !SAFE_RUNTIME_REVISION.test(manifest.runtime_revision)) {
    throw new Error('Document runtime revision is invalid');
  }
  const macos = args.platform.startsWith('macos-');
  const expectedSoffice = macos
    ? /^libreoffice\/(?:LibreOffice|LibreOfficeDev)\.app\/Contents\/MacOS\/soffice$/
    : /^libreoffice\/program\/soffice\.exe$/;
  const expectedPdftoppm = macos
    ? /^poppler\/bin\/pdftoppm$/
    : /^poppler\/(?:Library\/)?bin\/pdftoppm\.exe$/;

  async function verifyManifestFile(entry, label, expectedPath) {
    if (!entry || typeof entry !== 'object' || typeof entry.path !== 'string'
      || typeof entry.sha256 !== 'string' || typeof entry.version !== 'string') {
      throw new Error(`${label} manifest entry is incomplete`);
    }
    normalizeRelativePath(entry.path, `${label} path`);
    if (!expectedPath.test(entry.path) || !SAFE_SHA256.test(entry.sha256) || entry.version.trim().length === 0) {
      throw new Error(`${label} manifest entry is invalid for ${args.platform}`);
    }
    const file = requireRegularFile(runtimeRoot, entry.path, label, { executable: macos });
    const actualSha256 = await sha256File(file.absolute);
    if (actualSha256 !== entry.sha256) {
      throw new Error(`${label} SHA-256 does not match runtime.json`);
    }
    return { path: entry.path, bytes: file.size, sha256: actualSha256, version: entry.version };
  }

  const soffice = await verifyManifestFile(manifest.soffice, 'LibreOffice soffice', expectedSoffice);
  const pdftoppm = await verifyManifestFile(manifest.pdftoppm, 'Poppler pdftoppm', expectedPdftoppm);
  if (manifest.font_directory !== 'fonts' || !Array.isArray(manifest.fonts) || manifest.fonts.length !== 1) {
    throw new Error('Document runtime must contain the pinned fallback font contract');
  }
  const font = manifest.fonts[0];
  if (!font || font.path !== 'fonts/NotoSansSC-Regular.ttf' || !SAFE_SHA256.test(font.sha256)) {
    throw new Error('Document runtime fallback font entry is invalid');
  }
  const fontFile = requireRegularFile(runtimeRoot, font.path, 'Document fallback font');
  const fontSha256 = await sha256File(fontFile.absolute);
  if (fontSha256 !== font.sha256) {
    throw new Error('Document fallback font SHA-256 does not match runtime.json');
  }
  requireRegularFile(runtimeRoot, 'fonts/NotoSansSC-OFL.txt', 'Document fallback font license');
  if (macos) {
    if (manifest.poppler_library_dir !== 'poppler/lib') {
      throw new Error('macOS document runtime Poppler library directory is invalid');
    }
    requireDirectory(runtimeRoot, manifest.poppler_library_dir, 'Poppler library directory');
  } else if (manifest.poppler_library_dir !== null) {
    throw new Error('Windows document runtime must not declare a Poppler library directory override');
  }
  return {
    schema_version: manifest.schema_version,
    runtime_revision: manifest.runtime_revision,
    platform: manifest.platform,
    soffice,
    pdftoppm,
    fallback_font: { path: font.path, bytes: fontFile.size, sha256: fontSha256 },
  };
}

async function verifySkillAndPluginBundles(args) {
  const sourcePluginCatalog = readJson(args.pluginCatalog, 'Source Plugin catalog');
  const sourceSkillCatalog = readJson(args.skillCatalog, 'Source Skill catalog');
  const packagedCatalogRelative = 'skill-bundles/catalog/internal-skill-catalog.json';
  const packagedCatalogPath = requireRegularFile(args.resources, packagedCatalogRelative, 'Packaged Skill catalog').absolute;
  const packagedSkillCatalog = readJson(packagedCatalogPath, 'Packaged Skill catalog');
  if (sourcePluginCatalog.schema_version !== 1 || !Array.isArray(sourcePluginCatalog.plugins)
    || sourcePluginCatalog.plugins.length !== EXPECTED_PLUGIN_COUNT) {
    throw new Error(`Source Plugin catalog must contain exactly ${EXPECTED_PLUGIN_COUNT} schema-v1 entries`);
  }
  if (sourceSkillCatalog.schema_version !== 1 || !Array.isArray(sourceSkillCatalog.skills)
    || sourceSkillCatalog.skills.length !== EXPECTED_SKILL_COUNT) {
    throw new Error(`Source Skill catalog must contain exactly ${EXPECTED_SKILL_COUNT} schema-v1 entries`);
  }
  if (stableJson(sourceSkillCatalog) !== stableJson(packagedSkillCatalog)) {
    throw new Error('Packaged Skill catalog differs from the release source catalog');
  }
  if (!sourcePluginCatalog.catalog_revision
    || sourcePluginCatalog.catalog_revision !== sourceSkillCatalog.catalog_revision) {
    throw new Error('Plugin and Skill catalog revisions do not match');
  }
  const sourceSkillRoot = path.resolve(path.dirname(args.skillCatalog), '..', 'internal');
  const packagedSkillRoot = requireDirectory(args.resources, 'skill-bundles/internal', 'Packaged Skill root');
  const seenSkillIds = new Set();
  for (const skill of sourceSkillCatalog.skills) {
    if (!skill || typeof skill.skill_id !== 'string' || typeof skill.name !== 'string' || typeof skill.version !== 'string') {
      throw new Error('Skill catalog contains an invalid entry');
    }
    if (seenSkillIds.has(skill.skill_id)) {
      throw new Error(`Skill catalog contains duplicate id: ${skill.skill_id}`);
    }
    seenSkillIds.add(skill.skill_id);
    const bundleRelative = `${skill.name}/${skill.version}`;
    for (const fileName of ['skill.json', 'instructions.md']) {
      const relativePath = `${bundleRelative}/${fileName}`;
      const sourceFile = requireRegularFile(sourceSkillRoot, relativePath, `Source Skill ${skill.skill_id}`).absolute;
      const packagedFile = requireRegularFile(packagedSkillRoot, relativePath, `Packaged Skill ${skill.skill_id}`).absolute;
      if (await sha256File(sourceFile) !== await sha256File(packagedFile)) {
        throw new Error(`Packaged Skill differs from its release source: ${skill.skill_id}/${fileName}`);
      }
    }
  }
  const pluginRoot = requireDirectory(args.resources, 'plugin-bundles', 'Packaged Plugin Bundles');
  const indexPath = requireRegularFile(pluginRoot, 'plugin-bundle-index.json', 'Packaged Plugin Bundle index').absolute;
  const index = readJson(indexPath, 'Packaged Plugin Bundle index');
  if (index.schema_version !== 1 || index.platform !== args.platform
    || index.catalog_revision !== sourcePluginCatalog.catalog_revision
    || !Array.isArray(index.plugins) || index.plugins.length !== EXPECTED_PLUGIN_COUNT
    || index.plugins.flatMap((plugin) => plugin.skills || []).length !== EXPECTED_SKILL_COUNT) {
    throw new Error('Packaged Plugin Bundle index does not match the release catalogs/platform');
  }
  const bundleTool = path.join(SCRIPT_DIR, 'prepare-plugin-bundles.mjs');
  requireRegularFile(SCRIPT_DIR, 'prepare-plugin-bundles.mjs', 'Plugin Bundle verification tool');
  const verification = spawnSync(process.execPath, [
    bundleTool,
    '--verify-only',
    '--plugin-catalog', args.pluginCatalog,
    '--skill-catalog', packagedCatalogPath,
    '--skill-root', packagedSkillRoot,
    '--output', pluginRoot,
    '--platform', args.platform,
  ], { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 });
  if (verification.error || verification.status !== 0) {
    const detail = String(verification.stderr || verification.stdout || verification.error?.message || '').trim();
    throw new Error(`Packaged Plugin Bundle verification failed${detail ? `: ${detail}` : ''}`);
  }
  return {
    catalog_revision: sourcePluginCatalog.catalog_revision,
    release_version: sourcePluginCatalog.release_version,
    plugins: index.plugins.length,
    skills: EXPECTED_SKILL_COUNT,
    index_sha256: await sha256File(indexPath),
  };
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
    /CHATOS_DOCUMENT_RUNTIME_DIR:\s*path\.join\(browserRuntime\.toolsDir,\s*'documents-runtime'\)/,
    /CHATOS_BUNDLED_SKILLS_DIR:\s*resourcePath\('skill-bundles'\)/,
    /CHATOS_BUNDLED_PLUGINS_DIR:\s*resourcePath\('plugin-bundles'\)/,
    /CHATOS_CHROME_NATIVE_HOST_PATH:\s*resourcePath\(chromeNativeHostName\)/,
    /CHATOS_CHROME_EXTENSION_DIR:\s*chromeExtensionPath\(\)/,
    /env\.AGENT_BROWSER_BIN\s*=\s*browserRuntime\.agentBrowser/,
    /env\.AGENT_BROWSER_EXECUTABLE_PATH\s*=\s*browserRuntime\.browserExecutable/,
  ];
  if (requiredPatterns.some((pattern) => !pattern.test(runtime))) {
    throw new Error('Packaged Electron core runtime is missing a required packaged-resource binding');
  }
  if (!/process\.platform === 'darwin'[\s\S]{0,400}CHATOS_COMPUTER_USE_HELPER_PATH\s*=\s*resourcePath\(computerUseHelperName\)/.test(runtime)
    || !/app\.isPackaged[\s\S]{0,200}CHATOS_COMPUTER_USE_HELPER_REQUIRE_SIGNED\s*=\s*'1'/.test(runtime)) {
    throw new Error('Packaged Electron core runtime is missing the macOS signed Computer Use helper contract');
  }
  return { packaged_path: candidates[0], bytes: fs.lstatSync(packaged).size, sha256: packagedHash };
}

function verifyLinuxCoreRuntimeProfile(args) {
  const forbiddenPaths = [
    'chrome-extension',
    'chatos_chrome_native_host',
    'chatos_computer_use_helper',
    `bundled-tools/${args.platform}/agent-browser`,
    `bundled-tools/${args.platform}/browser`,
    `bundled-tools/${args.platform}/documents-runtime`,
  ];
  for (const relativePath of forbiddenPaths) {
    const absolutePath = path.join(args.resources, ...relativePath.split('/'));
    if (fs.existsSync(absolutePath)) {
      throw new Error(`Linux core package contains an unsupported full-runtime resource: ${relativePath}`);
    }
  }
  return {
    chrome_extension: { verified: false, reason: 'not included in the linux-core runtime profile' },
    browser_runtime: { verified: false, reason: 'not included in the linux-core runtime profile' },
    document_runtime: { verified: false, reason: 'not included in the linux-core runtime profile' },
  };
}

async function verifyLinuxBrowserRuntimeProfile(args) {
  const forbiddenPaths = [
    'chatos_computer_use_helper',
    `bundled-tools/${args.platform}/agent-browser`,
    `bundled-tools/${args.platform}/browser`,
    `bundled-tools/${args.platform}/documents-runtime`,
  ];
  for (const relativePath of forbiddenPaths) {
    const absolutePath = path.join(args.resources, ...relativePath.split('/'));
    if (fs.existsSync(absolutePath)) {
      throw new Error(`Linux browser package contains an unsupported full-runtime resource: ${relativePath}`);
    }
  }
  return {
    chrome_extension: await verifyChromeExtension(args),
    browser_runtime: { verified: false, reason: 'bundled browser automation runtime is not included in the linux-browser profile' },
    document_runtime: { verified: false, reason: 'bundled document runtime is not included in the linux-browser profile' },
  };
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
    [args?.pluginCatalog, '<plugin-catalog>'],
    [args?.skillCatalog, '<skill-catalog>'],
    [args?.electronRuntimeSource, '<electron-runtime-source>'],
    [args?.chromeExtensionSource, '<chrome-extension-source>'],
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
  const tree = scanResourceTree(args.resources);
  requireDirectory(args.resources, 'chatos-frontend', 'Bundled ChatOS frontend');
  requireRegularFile(args.resources, 'chatos-frontend/index.html', 'Bundled ChatOS frontend entrypoint');
  const migrations = requireDirectory(args.resources, 'sqlite-migrations', 'SQLite migrations');

  const packageArchitecture = args.platform.endsWith('-arm64') ? 'arm64' : 'x64';
  const macos = args.platform.startsWith('macos-');
  const windows = args.platform.startsWith('windows-');
  const linuxBrowser = args.runtimeProfile === 'linux-browser';
  const binarySpecs = macos
    ? [
        ['local_connector_client_core', 'Local Connector Core'],
        ['chatos_chrome_native_host', 'Chrome Native Messaging Host'],
        ['chatos_computer_use_helper', 'Computer Use helper'],
        ['chatos_sandbox_mcp_server', 'Sandbox MCP server'],
      ]
    : windows
      ? [
        ['local_connector_client_core.exe', 'Local Connector Core'],
        ['chatos_chrome_native_host.exe', 'Chrome Native Messaging Host'],
        ['chatos_sandbox_mcp_server.exe', 'Sandbox MCP server'],
      ]
      : [
        ['local_connector_client_core', 'Local Connector Core'],
        ...(linuxBrowser ? [['chatos_chrome_native_host', 'Chrome Native Messaging Host']] : []),
        ['chatos_sandbox_mcp_server', 'Sandbox MCP server'],
      ];
  const executables = [];
  const executablePaths = [];
  for (const [relativePath, label] of binarySpecs) {
    const verified = await verifyBinary(args.resources, relativePath, label, args.platform, packageArchitecture);
    executables.push(verified);
    executablePaths.push(path.join(args.resources, relativePath));
  }
  const localRuntimeMigrations = verifyEmbeddedSqliteMigrations(args, executablePaths[0], migrations);

  let runtimeVerification;
  if (args.runtimeProfile === 'full') {
    runtimeVerification = Promise.all([
        verifyChromeExtension(args),
        verifyBrowserRuntime(args),
        verifyDocumentRuntime(args),
      ]).then(([chromeExtension, browserRuntime, documentRuntime]) => ({
        chrome_extension: chromeExtension,
        browser_runtime: browserRuntime,
        document_runtime: documentRuntime,
      }));
  } else if (args.runtimeProfile === 'linux-browser') {
    runtimeVerification = verifyLinuxBrowserRuntimeProfile(args);
  } else {
    runtimeVerification = Promise.resolve(verifyLinuxCoreRuntimeProfile(args));
  }
  const [runtime, bundles, electronRuntime] = await Promise.all([
    runtimeVerification,
    verifySkillAndPluginBundles(args),
    verifyElectronRuntime(args),
  ]);
  const codeSigning = verifyMacCodeSigning(args, executablePaths);
  const report = {
    schema_version: 1,
    result: 'verified',
    verified_at: new Date().toISOString(),
    platform: args.platform,
    runtime_profile: args.runtimeProfile,
    resource_tree: tree,
    executables,
    chrome_extension: runtime.chrome_extension,
    browser_runtime: runtime.browser_runtime,
    document_runtime: runtime.document_runtime,
    local_runtime_migrations: localRuntimeMigrations,
    plugin_bundles: bundles,
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

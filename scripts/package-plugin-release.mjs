// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const [, , mode, inputPath, ...rest] = process.argv;
if (!['--verify', '--write-metadata', '--package'].includes(mode) || !inputPath) {
  throw new Error('usage: node scripts/package-plugin-release.mjs --verify|--write-metadata|--package <plugin-dir> [--output <zip>]');
}

const root = resolve(inputPath);
const manifestPath = join(root, '.chatos-plugin', 'plugin.json');
if (!existsSync(manifestPath)) throw new Error('missing .chatos-plugin/plugin.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
if (![1, 2].includes(manifest.schemaVersion)) throw new Error('unsupported Plugin schemaVersion');
if (manifest.schemaVersion === 2 && !manifest.execution?.defaultHost) throw new Error('schemaVersion 2 requires execution.defaultHost');

if (mode === '--write-metadata') writeMetadata(root, manifest);
verifyMetadata(root);
normalizedManifestSha256(root);
if (mode === '--package') packageRelease(root, manifest, outputArgument(rest));

function packageRelease(sourceRoot, pluginManifest, output) {
  const staging = mkdtempSync(join(tmpdir(), 'chatos-plugin-package-'));
  const packageRoot = join(staging, 'package');
  try {
    cpSync(sourceRoot, packageRoot, { recursive: true, dereference: false });
    writeMetadata(packageRoot, pluginManifest);
    verifyMetadata(packageRoot);
    const epoch = new Date('2025-01-01T00:00:00.000Z');
    for (const path of listFiles(packageRoot, true)) utimesSync(join(packageRoot, path), epoch, epoch);
    const outputPath = resolve(output || join('dist', 'plugins', `${pluginManifest.name}-${pluginManifest.version}.zip`));
    mkdirSync(dirname(outputPath), { recursive: true });
    rmSync(outputPath, { force: true });
    execFileSync('zip', ['-X', '-q', outputPath, ...listFiles(packageRoot, false)], { cwd: packageRoot });
    const artifactSha256 = sha256(readFileSync(outputPath));
    const manifestSha256 = normalizedManifestSha256(packageRoot);
    const releasePayload = {
      plugin_id: '<catalog-plugin-id>',
      version: pluginManifest.version,
      artifact_ref: `<publish-url>/${basename(outputPath)}`,
      artifact_sha256: artifactSha256,
      sbom_ref: './sbom.spdx.json',
      signature: {
        key_id: '<release-signing-key-id>',
        publisher_id: '<publisher-id>',
        marketplace_id: '<marketplace-id>',
        algorithm: 'ed25519',
        signature_base64: '<production-ed25519-signature-required>',
        signed_at: '<RFC3339-signing-time>',
        manifest_sha256: manifestSha256,
      },
    };
    writeFileSync(`${outputPath}.release.json`, `${JSON.stringify(releasePayload, null, 2)}\n`);
    process.stdout.write(`${outputPath}\n${artifactSha256}\n`);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

function writeMetadata(packageRoot, pluginManifest) {
  const sbom = {
    spdxVersion: 'SPDX-2.3',
    dataLicense: 'CC0-1.0',
    SPDXID: 'SPDXRef-DOCUMENT',
    name: `${pluginManifest.name}-${pluginManifest.version}`,
    documentNamespace: `https://chatos.dev/spdx/${pluginManifest.name}/${pluginManifest.version}`,
    creationInfo: { created: '2025-01-01T00:00:00Z', creators: ['Tool: ChatOS package-plugin-release.mjs'] },
    packages: [{
      name: pluginManifest.name,
      SPDXID: 'SPDXRef-Package',
      versionInfo: pluginManifest.version,
      downloadLocation: 'NOASSERTION',
      filesAnalyzed: true,
      licenseConcluded: pluginManifest.license || 'NOASSERTION',
      licenseDeclared: pluginManifest.license || 'NOASSERTION',
      copyrightText: 'Copyright (c) 2026 DietrichGebert; ChatOS adaptation copyright (c) 2025 AI Chat Team',
    }],
  };
  writeFileSync(join(packageRoot, 'sbom.spdx.json'), `${JSON.stringify(sbom, null, 2)}\n`);
  const checksumPath = join(packageRoot, '.chatos-plugin', 'checksums.json');
  const files = Object.fromEntries(listFiles(packageRoot, false)
    .filter((path) => path !== '.chatos-plugin/checksums.json')
    .map((path) => [path, sha256(readFileSync(join(packageRoot, path)))]));
  writeFileSync(checksumPath, `${JSON.stringify({ schemaVersion: 1, files }, null, 2)}\n`);
}

function verifyMetadata(packageRoot) {
  const checksumPath = join(packageRoot, '.chatos-plugin', 'checksums.json');
  if (!existsSync(checksumPath)) throw new Error('missing .chatos-plugin/checksums.json; run --write-metadata');
  const index = JSON.parse(readFileSync(checksumPath, 'utf8'));
  if (index.schemaVersion !== 1 || !index.files || Array.isArray(index.files)) throw new Error('invalid checksum index');
  const actual = listFiles(packageRoot, false).filter((path) => path !== '.chatos-plugin/checksums.json');
  const expected = Object.keys(index.files).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error('checksum index must cover every package file exactly once');
  for (const path of actual) {
    if (index.files[path] !== sha256(readFileSync(join(packageRoot, path)))) throw new Error(`checksum mismatch: ${path}`);
  }
  const sbom = JSON.parse(readFileSync(join(packageRoot, 'sbom.spdx.json'), 'utf8'));
  if (!String(sbom.spdxVersion || '').startsWith('SPDX-') || !sbom.SPDXID) throw new Error('invalid SPDX SBOM');
}

function listFiles(packageRoot, includeDirectories) {
  const output = [];
  const walk = (directory) => {
    for (const name of readdirSync(directory).sort()) {
      if (['.git', 'node_modules', 'target', 'dist', '__pycache__'].includes(name)) throw new Error(`forbidden package path: ${name}`);
      const fullPath = join(directory, name);
      const info = lstatSync(fullPath);
      if (info.isSymbolicLink() || (!info.isDirectory() && !info.isFile())) throw new Error(`unsafe package entry: ${relative(packageRoot, fullPath)}`);
      const path = relative(packageRoot, fullPath).replaceAll('\\', '/');
      if (info.isDirectory()) {
        if (includeDirectories) output.push(path);
        walk(fullPath);
      } else {
        if (statSync(fullPath).size > 2 * 1024 * 1024) throw new Error(`package file exceeds 2 MiB: ${path}`);
        output.push(path);
      }
    }
  };
  walk(packageRoot);
  return output.sort();
}

function outputArgument(args) {
  const index = args.indexOf('--output');
  return index >= 0 ? args[index + 1] : null;
}

function normalizedManifestSha256(packageRoot) {
  const path = join(packageRoot, '.chatos-plugin', 'plugin.json');
  return execFileSync('cargo', [
    'run',
    '--quiet',
    '-p',
    'chatos_plugin_package',
    '--bin',
    'normalized-plugin-manifest-sha256',
    '--',
    path,
  ], {
    cwd: repositoryRoot,
    encoding: 'utf8',
  }).trim();
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

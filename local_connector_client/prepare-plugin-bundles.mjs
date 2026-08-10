#!/usr/bin/env node
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const MAX_BUNDLE_FILES = 4096;
const MAX_BUNDLE_FILE_BYTES = 16 * 1024 * 1024;
const SAFE_NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const SAFE_PLATFORM = /^(?:macos|windows|linux)-(?:arm64|x64)$/;
const SAFE_VERSION = /^[0-9]+\.[0-9]+\.[0-9]+$/;
const SAFE_ARTIFACT_REVISION = /^[0-9A-Za-z][0-9A-Za-z._-]{0,119}$/;
const SAFE_RELEASE_EPOCH = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;

function isPlatformMetadataName(name) {
  return name === '.DS_Store' || name.startsWith('._');
}

function platformMetadataPaths(root) {
  const metadataPaths = [];
  if (!fs.existsSync(root)) {
    return metadataPaths;
  }
  function visit(relativeRoot) {
    const absoluteRoot = path.join(root, relativeRoot);
    for (const entry of fs.readdirSync(absoluteRoot)) {
      const relativePath = path.join(relativeRoot, entry);
      const absolutePath = path.join(root, relativePath);
      if (isPlatformMetadataName(entry)) {
        metadataPaths.push(relativePath);
        continue;
      }
      if (fs.lstatSync(absolutePath).isDirectory()) {
        visit(relativePath);
      }
    }
  }
  visit('');
  return metadataPaths;
}

function removePlatformMetadata(root) {
  for (const relativePath of platformMetadataPaths(root).reverse()) {
    fs.rmSync(path.join(root, relativePath), { recursive: true, force: true });
  }
}

function assertNoPlatformMetadata(root) {
  const metadataPaths = platformMetadataPaths(root);
  if (metadataPaths.length > 0) {
    throw new Error([
      `Staged Plugin Bundle contains macOS filesystem metadata: ${root}`,
      ...metadataPaths.slice(0, 20).map((relativePath) => `  unexpected: ${relativePath.split(path.sep).join('/')}`),
    ].join('\n'));
  }
}

function parseArgs(argv) {
  const args = { verifyOnly: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--verify-only') {
      args.verifyOnly = true;
      continue;
    }
    if (!argument.startsWith('--') || index + 1 >= argv.length) {
      throw new Error(`Invalid argument: ${argument}`);
    }
    args[argument.slice(2).replace(/-([a-z])/g, (_, char) => char.toUpperCase())] = argv[++index];
  }
  for (const required of ['pluginCatalog', 'skillCatalog', 'skillRoot', 'output', 'platform']) {
    if (!args[required]) {
      throw new Error(`Missing required --${required.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`)}`);
    }
  }
  if (!SAFE_PLATFORM.test(args.platform)) {
    throw new Error(`Unsupported Plugin Bundle platform: ${args.platform}`);
  }
  return args;
}

function readJson(filePath, label) {
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    throw new Error(`Read ${label} failed at ${filePath}: ${error.message}`);
  }
  return parsed;
}

function sha256(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

function prettyJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function bundledSkillDocument(skill) {
  const instructions = fs.readFileSync(path.join(skill.bundleRoot, 'instructions.md'), 'utf8');
  return [
    '---',
    `name: ${skill.name}`,
    `description: ${JSON.stringify(skill.description || skill.display_name || skill.name)}`,
    'disable-model-invocation: false',
    '---',
    '',
    instructions.trimEnd(),
    '',
  ].join('\n');
}

function assertString(value, label) {
  if (typeof value !== 'string' || value.trim() !== value || value.length === 0) {
    throw new Error(`${label} must be a non-empty trimmed string`);
  }
}

function assertReleaseMetadata(version, releaseEpoch, artifactRevision, label) {
  if (!SAFE_VERSION.test(version)) {
    throw new Error(`${label} release version is invalid: ${version}`);
  }
  if (!SAFE_RELEASE_EPOCH.test(releaseEpoch) || Number.isNaN(Date.parse(releaseEpoch))) {
    throw new Error(`${label} release epoch is invalid: ${releaseEpoch}`);
  }
  if (!SAFE_ARTIFACT_REVISION.test(artifactRevision)) {
    throw new Error(`${label} artifact revision is invalid: ${artifactRevision}`);
  }
}

function sourceContext(args) {
  const pluginCatalog = readJson(args.pluginCatalog, 'bundled Plugin catalog');
  const skillCatalog = readJson(args.skillCatalog, 'internal Skill catalog');
  if (pluginCatalog.schema_version !== 1 || !Array.isArray(pluginCatalog.plugins)) {
    throw new Error('Bundled Plugin catalog must be a schema-v1 document');
  }
  if (skillCatalog.schema_version !== 1 || !Array.isArray(skillCatalog.skills)) {
    throw new Error('Internal Skill catalog must be a schema-v1 document');
  }
  if (pluginCatalog.catalog_revision !== skillCatalog.catalog_revision) {
    throw new Error('Bundled Plugin and internal Skill catalog revisions do not match');
  }
  assertString(pluginCatalog.release_version, 'Bundled Plugin release version');
  assertString(pluginCatalog.release_epoch, 'Bundled Plugin release epoch');
  assertString(pluginCatalog.artifact_revision, 'Bundled Plugin artifact revision');
  assertReleaseMetadata(
    pluginCatalog.release_version,
    pluginCatalog.release_epoch,
    pluginCatalog.artifact_revision,
    'Bundled Plugin default',
  );
  const skillsById = new Map();
  for (const skill of skillCatalog.skills) {
    for (const field of ['skill_id', 'bundle_id', 'version', 'name', 'entrypoint_kind']) {
      assertString(skill[field], `Skill ${field}`);
    }
    if (!SAFE_NAME.test(skill.name) || !Array.isArray(skill.permissions)) {
      throw new Error(`Internal Skill catalog contains an invalid entry: ${skill.skill_id}`);
    }
    if (skillsById.has(skill.skill_id)) {
      throw new Error(`Internal Skill catalog contains duplicate id: ${skill.skill_id}`);
    }
    const bundleRoot = path.resolve(args.skillRoot, skill.name, skill.version);
    const manifestPath = path.join(bundleRoot, 'skill.json');
    const instructionsPath = path.join(bundleRoot, 'instructions.md');
    const manifest = readJson(manifestPath, `Skill manifest ${skill.skill_id}`);
    for (const field of ['skill_id', 'bundle_id', 'version', 'name']) {
      if (manifest[field] !== skill[field]) {
        throw new Error(`Skill ${skill.skill_id} ${field} does not match its catalog entry`);
      }
    }
    const manifestBytes = fs.readFileSync(manifestPath);
    const instructionsBytes = fs.readFileSync(instructionsPath);
    const payload = [
      'chatos-internal-skill-bundle-v2',
      skill.skill_id,
      skill.bundle_id,
      skill.version,
      skill.entrypoint_kind,
      skill.implementation_status,
      sha256(instructionsBytes),
      sha256(manifestBytes),
      String(Boolean(skill.requires_workspace)),
      skill.permissions.join(','),
    ].join('\n');
    skillsById.set(skill.skill_id, {
      ...skill,
      bundleRoot,
      bundle_hash: sha256(payload),
    });
  }

  const seenPlugins = new Set();
  const seenSkills = new Set();
  for (const plugin of pluginCatalog.plugins) {
    for (const field of ['name', 'display_name', 'description', 'category']) {
      assertString(plugin[field], `Plugin ${field}`);
    }
    if (!SAFE_NAME.test(plugin.name) || !Array.isArray(plugin.skill_ids) || plugin.skill_ids.length === 0) {
      throw new Error(`Bundled Plugin catalog contains an invalid entry: ${plugin.name}`);
    }
    plugin.release_version = plugin.release_version || pluginCatalog.release_version;
    plugin.release_epoch = plugin.release_epoch || pluginCatalog.release_epoch;
    plugin.artifact_revision = plugin.artifact_revision || pluginCatalog.artifact_revision;
    assertString(plugin.release_version, `Plugin ${plugin.name} release_version`);
    assertString(plugin.release_epoch, `Plugin ${plugin.name} release_epoch`);
    assertString(plugin.artifact_revision, `Plugin ${plugin.name} artifact_revision`);
    assertReleaseMetadata(
      plugin.release_version,
      plugin.release_epoch,
      plugin.artifact_revision,
      `Bundled Plugin ${plugin.name}`,
    );
    if (seenPlugins.has(plugin.name)) {
      throw new Error(`Bundled Plugin catalog contains duplicate name: ${plugin.name}`);
    }
    seenPlugins.add(plugin.name);
    for (const skillId of plugin.skill_ids) {
      if (!skillsById.has(skillId)) {
        throw new Error(`Bundled Plugin ${plugin.name} references unknown Skill: ${skillId}`);
      }
      if (seenSkills.has(skillId)) {
        throw new Error(`Internal Skill is mapped by more than one Plugin: ${skillId}`);
      }
      seenSkills.add(skillId);
    }
  }
  if (seenSkills.size !== skillsById.size) {
    const missing = [...skillsById.keys()].filter((skillId) => !seenSkills.has(skillId));
    throw new Error(`Internal Skills missing bundled Plugin mapping: ${missing.join(', ')}`);
  }
  return { pluginCatalog, skillCatalog, skillsById };
}

function normalizedManifest(plugin, skills, releaseVersion) {
  const permissionComponents = new Map();
  for (const skill of skills) {
    for (const permission of skill.permissions) {
      const components = permissionComponents.get(permission) || [];
      components.push(skill.name);
      permissionComponents.set(permission, components);
    }
  }
  const permissions = [...permissionComponents.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([permission, components]) => ({
      permission,
      required: true,
      reason: 'Required by bundled Skill components',
      components: [...new Set(components)].sort(),
    }));
  const capabilities = [...new Set(skills.map((skill) => skill.entrypoint_kind))].sort();
  return {
    schemaVersion: 1,
    name: plugin.name,
    version: releaseVersion,
    description: plugin.description,
    author: { name: 'ChatOS', email: null, url: null },
    homepage: null,
    repository: null,
    license: 'LicenseRef-Pending-Redistribution-Review',
    keywords: ['bundled', 'skills'],
    skills: skills.map((skill) => `./skills/${skill.name}`),
    mcpServers: [],
    apps: [],
    commands: [],
    agents: [],
    hooks: [],
    ui: [],
    interface: {
      displayName: plugin.display_name,
      shortDescription: plugin.description,
      longDescription: plugin.description,
      developerName: 'ChatOS',
      category: plugin.category,
      capabilities: capabilities.length > 0 ? capabilities : ['skills'],
      websiteURL: null,
      privacyPolicyURL: null,
      termsOfServiceURL: null,
      defaultPrompt: [],
      brandColor: null,
      composerIcon: null,
      logo: null,
      logoDark: null,
      screenshots: [],
    },
    dependencies: {
      minimumHostVersion: null,
      plugins: [],
      executables: [],
      supportedPlatforms: [],
    },
    permissions,
    bundledContentVariant: 'chatos-internal-skill-bundles-v2',
  };
}

function shouldIncludePlatformPath(relativePath, platform) {
  const parts = relativePath.split('/');
  if (!['binaries', 'platforms'].includes(parts[0]) || parts.length < 2) {
    return true;
  }
  return !SAFE_PLATFORM.test(parts[1]) || parts[1] === platform;
}

function copyBundleTree(sourceRoot, destinationRoot, platform, relativeRoot = '') {
  const source = path.join(sourceRoot, relativeRoot);
  const stat = fs.lstatSync(source);
  if (stat.isSymbolicLink() || (!stat.isDirectory() && !stat.isFile())) {
    throw new Error(`Plugin Bundle source contains a symlink or special file: ${source}`);
  }
  if (!shouldIncludePlatformPath(relativeRoot.replaceAll(path.sep, '/'), platform)) {
    return;
  }
  const destination = path.join(destinationRoot, relativeRoot);
  if (stat.isDirectory()) {
    fs.mkdirSync(destination, { recursive: true });
    for (const entry of fs.readdirSync(source).sort()) {
      if (isPlatformMetadataName(entry)) {
        continue;
      }
      copyBundleTree(sourceRoot, destinationRoot, platform, path.join(relativeRoot, entry));
    }
    return;
  }
  if (stat.size > MAX_BUNDLE_FILE_BYTES) {
    throw new Error(`Plugin Bundle source file exceeds 16 MiB: ${source}`);
  }
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
}

function listFiles(root) {
  const files = [];
  function visit(relativeRoot) {
    const absoluteRoot = path.join(root, relativeRoot);
    for (const entry of fs.readdirSync(absoluteRoot).sort()) {
      const relativePath = path.join(relativeRoot, entry);
      const absolutePath = path.join(root, relativePath);
      const stat = fs.lstatSync(absolutePath);
      if (stat.isSymbolicLink() || (!stat.isDirectory() && !stat.isFile())) {
        throw new Error(`Staged Plugin Bundle contains a symlink or special file: ${absolutePath}`);
      }
      if (stat.isDirectory()) {
        visit(relativePath);
      } else {
        if (stat.size > MAX_BUNDLE_FILE_BYTES) {
          throw new Error(`Staged Plugin Bundle file exceeds 16 MiB: ${absolutePath}`);
        }
        files.push(relativePath.split(path.sep).join('/'));
      }
    }
  }
  visit('');
  if (files.length > MAX_BUNDLE_FILES) {
    throw new Error(`Staged Plugin Bundle exceeds ${MAX_BUNDLE_FILES} files: ${root}`);
  }
  return files;
}

function fileChecksums(root, excluded = new Set()) {
  const checksums = {};
  for (const relativePath of listFiles(root)) {
    if (!excluded.has(relativePath)) {
      checksums[relativePath] = sha256(fs.readFileSync(path.join(root, relativePath)));
    }
  }
  return checksums;
}

function checksumMismatchDetails(expected, actual) {
  const differences = [];
  const paths = [...new Set([...Object.keys(expected), ...Object.keys(actual)])].sort();
  for (const relativePath of paths) {
    if (!(relativePath in expected)) {
      differences.push(`  unexpected: ${relativePath}`);
    } else if (!(relativePath in actual)) {
      differences.push(`  missing: ${relativePath}`);
    } else if (expected[relativePath] !== actual[relativePath]) {
      differences.push(`  changed: ${relativePath}`);
    }
  }
  return differences.slice(0, 20);
}

function pluginArtifactHash(pluginName, artifactRevision, skills) {
  const parts = skills
    .map((skill) => `${skill.skill_id}:${skill.bundle_hash}`)
    .sort();
  return sha256([
    'chatos-bundled-plugin-release-v1',
    pluginName,
    artifactRevision,
    ...parts,
  ].join('\n'));
}

function sbomDocument(plugin, skills, releaseVersion, releaseEpoch, artifactHash) {
  return {
    spdxVersion: 'SPDX-2.3',
    dataLicense: 'CC0-1.0',
    SPDXID: 'SPDXRef-DOCUMENT',
    name: `ChatOS bundled Plugin ${plugin.name} ${releaseVersion}`,
    documentNamespace: `https://chatos.local/spdx/${plugin.name}/${releaseVersion}/${artifactHash}`,
    creationInfo: {
      created: releaseEpoch,
      creators: ['Organization: ChatOS'],
    },
    packages: skills.map((skill, index) => ({
      name: skill.bundle_id,
      SPDXID: `SPDXRef-Package-${index + 1}`,
      versionInfo: skill.version,
      downloadLocation: 'NOASSERTION',
      filesAnalyzed: false,
      licenseConcluded: 'NOASSERTION',
      licenseDeclared: 'NOASSERTION',
      checksums: [{ algorithm: 'SHA256', checksumValue: skill.bundle_hash }],
    })),
  };
}

function expectedPluginEntry(context, plugin, outputRoot, platform, stage) {
  const skills = plugin.skill_ids.map((skillId) => context.skillsById.get(skillId));
  const version = plugin.release_version;
  const relativePath = `internal/${plugin.name}/${version}`;
  const bundleRoot = path.join(outputRoot, ...relativePath.split('/'));
  const manifest = normalizedManifest(plugin, skills, version);
  const artifactHash = pluginArtifactHash(
    plugin.name,
    plugin.artifact_revision,
    skills,
  );
  if (stage) {
    fs.mkdirSync(path.join(bundleRoot, '.chatos-plugin'), { recursive: true });
    for (const skill of skills) {
      const stagedSkillRoot = path.join(bundleRoot, 'skills', skill.name);
      copyBundleTree(
        skill.bundleRoot,
        stagedSkillRoot,
        platform,
      );
      fs.writeFileSync(path.join(stagedSkillRoot, 'SKILL.md'), bundledSkillDocument(skill));
    }
    fs.writeFileSync(
      path.join(bundleRoot, '.chatos-plugin', 'plugin.json'),
      prettyJson(manifest),
    );
    fs.writeFileSync(
      path.join(bundleRoot, 'sbom.spdx.json'),
      prettyJson(sbomDocument(plugin, skills, version, plugin.release_epoch, artifactHash)),
    );
    // ExFAT stores macOS extended attributes as AppleDouble files beside the
    // real files. They are filesystem metadata, not distributable content.
    removePlatformMetadata(bundleRoot);
    const checksums = fileChecksums(
      bundleRoot,
      new Set(['.chatos-plugin/checksums.json']),
    );
    fs.writeFileSync(
      path.join(bundleRoot, '.chatos-plugin', 'checksums.json'),
      prettyJson({ schemaVersion: 1, files: checksums }),
    );
    removePlatformMetadata(bundleRoot);
  }
  verifyBundle(bundleRoot, manifest);
  const stagedChecksums = fileChecksums(bundleRoot);
  const contentPayload = [
    'chatos-staged-plugin-bundle-v1',
    ...Object.entries(stagedChecksums).map(([file, digest]) => `${file}:${digest}`),
  ].join('\n');
  return {
    plugin_id: `bundled-plugin-${plugin.name}`,
    release_id: bundledReleaseId(plugin.name, version),
    name: plugin.name,
    version,
    published_at: plugin.release_epoch,
    artifact_revision: plugin.artifact_revision,
    platform,
    relative_path: relativePath,
    manifest_sha256: sha256(JSON.stringify(manifest)),
    artifact_sha256: artifactHash,
    staged_content_sha256: sha256(contentPayload),
    skills: skills.map((skill) => ({
      skill_id: skill.skill_id,
      bundle_id: skill.bundle_id,
      name: skill.name,
      version: skill.version,
      bundle_hash: skill.bundle_hash,
    })),
  };
}

function bundledReleaseId(pluginName, version) {
  const normalizedVersion = version.replace(/[^0-9A-Za-z]/g, '-').toLowerCase();
  return `bundled-release-${pluginName}-${normalizedVersion}`;
}

function verifyBundle(bundleRoot, expectedManifest) {
  const manifestPath = path.join(bundleRoot, '.chatos-plugin', 'plugin.json');
  const checksumPath = path.join(bundleRoot, '.chatos-plugin', 'checksums.json');
  const sbomPath = path.join(bundleRoot, 'sbom.spdx.json');
  const manifest = readJson(manifestPath, 'staged Plugin Manifest');
  if (JSON.stringify(manifest) !== JSON.stringify(expectedManifest)) {
    throw new Error(`Staged Plugin Manifest drift detected: ${manifestPath}`);
  }
  const sbom = readJson(sbomPath, 'staged Plugin SBOM');
  if (!String(sbom.spdxVersion || '').startsWith('SPDX-') || !sbom.SPDXID) {
    throw new Error(`Staged Plugin SBOM is not SPDX JSON: ${sbomPath}`);
  }
  const checksumIndex = readJson(checksumPath, 'staged Plugin checksum index');
  if (checksumIndex.schemaVersion !== 1 || typeof checksumIndex.files !== 'object') {
    throw new Error(`Invalid staged Plugin checksum index: ${checksumPath}`);
  }
  const actual = fileChecksums(bundleRoot, new Set(['.chatos-plugin/checksums.json']));
  if (JSON.stringify(checksumIndex.files) !== JSON.stringify(actual)) {
    throw new Error([
      `Staged Plugin file checksum mismatch: ${bundleRoot}`,
      ...checksumMismatchDetails(checksumIndex.files, actual),
    ].join('\n'));
  }
}

function stageOrVerify(args) {
  const context = sourceContext(args);
  const outputRoot = path.resolve(args.output);
  if (!args.verifyOnly) {
    fs.rmSync(outputRoot, { recursive: true, force: true });
    fs.mkdirSync(outputRoot, { recursive: true });
  } else if (!fs.statSync(outputRoot).isDirectory()) {
    throw new Error(`Staged Plugin Bundle root is missing: ${outputRoot}`);
  } else {
    assertNoPlatformMetadata(outputRoot);
  }
  const plugins = context.pluginCatalog.plugins.map((plugin) => expectedPluginEntry(
    context,
    plugin,
    outputRoot,
    args.platform,
    !args.verifyOnly,
  ));
  const index = {
    schema_version: 1,
    catalog_revision: context.pluginCatalog.catalog_revision,
    release_version: context.pluginCatalog.release_version,
    release_epoch: context.pluginCatalog.release_epoch,
    artifact_revision: context.pluginCatalog.artifact_revision,
    platform: args.platform,
    plugins,
  };
  const indexPath = path.join(outputRoot, 'plugin-bundle-index.json');
  if (args.verifyOnly) {
    const existing = readJson(indexPath, 'staged Plugin Bundle index');
    if (JSON.stringify(existing) !== JSON.stringify(index)) {
      throw new Error(`Staged Plugin Bundle index drift detected: ${indexPath}`);
    }
  } else {
    fs.writeFileSync(indexPath, prettyJson(index));
    removePlatformMetadata(outputRoot);
    assertNoPlatformMetadata(outputRoot);
  }
  if (plugins.length !== 12 || plugins.flatMap((plugin) => plugin.skills).length !== 28) {
    throw new Error('Staged Plugin Bundle index must cover exactly 12 Plugins and 28 Skills');
  }
  process.stdout.write(`[OK] ${args.verifyOnly ? 'Verified' : 'Staged'} ${plugins.length} Plugin Bundles for ${args.platform}: ${outputRoot}\n`);
}

try {
  stageOrVerify(parseArgs(process.argv.slice(2)));
} catch (error) {
  process.stderr.write(`[ERROR] ${error.stack || error}\n`);
  process.exitCode = 1;
}

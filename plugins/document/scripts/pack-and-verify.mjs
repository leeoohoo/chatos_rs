import { execFileSync } from 'node:child_process';
import { chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const packageJson = JSON.parse(await readFile(path.join(projectRoot, 'package.json'), 'utf8'));
const manifest = JSON.parse(await readFile(path.join(projectRoot, 'chatos.plugin.json'), 'utf8'));
const pdfiumManifest = JSON.parse(await readFile(path.join(projectRoot, 'vendor', 'pdfium-v7243.json'), 'utf8'));
const pdfiumThirdPartyManifest = JSON.parse(await readFile(
  path.join(projectRoot, 'vendor', pdfiumManifest.review.thirdPartyManifest),
  'utf8'
));

function tarText(header, start, length) {
  return header.subarray(start, start + length).toString('utf8').replace(/\0.*$/s, '').trim();
}

function tarNumber(header, start, length) {
  const value = tarText(header, start, length);
  if (!/^[0-7]*$/.test(value)) throw new Error('packed artifact contains an unsupported tar number');
  return value ? Number.parseInt(value, 8) : 0;
}

function safeTarPath(name) {
  if (!name.startsWith('package/') || name.includes('\\') || path.posix.isAbsolute(name)) return false;
  return !name.split('/').some((component) => !component || component === '.' || component === '..');
}

async function extractTarball(tarballPath, destination) {
  const archive = gunzipSync(await readFile(tarballPath));
  let offset = 0;
  while (offset + 512 <= archive.length) {
    const header = archive.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const prefix = tarText(header, 345, 155);
    const basename = tarText(header, 0, 100);
    const name = prefix ? `${prefix}/${basename}` : basename;
    const size = tarNumber(header, 124, 12);
    const mode = tarNumber(header, 100, 8) & 0o777;
    const type = String.fromCharCode(header[156] || 48);
    const dataStart = offset + 512;
    const dataEnd = dataStart + size;
    if (dataEnd > archive.length) throw new Error('packed artifact tar data is truncated');
    if (type !== 'x' && type !== 'g') {
      const entryName = type === '5' ? name.replace(/\/+$/, '') : name;
      if (!safeTarPath(entryName)) throw new Error(`packed artifact contains an unsafe path: ${name}`);
      const target = path.join(destination, ...entryName.split('/'));
      if (type === '5') {
        await mkdir(target, { recursive: true });
      } else if (type === '0' || type === '\0') {
        await mkdir(path.dirname(target), { recursive: true });
        await writeFile(target, archive.subarray(dataStart, dataEnd), { flag: 'wx', mode: mode || 0o644 });
        await chmod(target, mode || 0o644);
      } else {
        throw new Error(`packed artifact contains an unsupported tar entry type: ${type}`);
      }
    }
    offset = dataStart + Math.ceil(size / 512) * 512;
  }
}

async function smokeTestPackedPackage(packageRoot, temporaryRoot) {
  const workspace = path.join(temporaryRoot, 'workspace');
  const artifact = path.join(temporaryRoot, 'artifacts');
  await mkdir(workspace);
  await mkdir(artifact);
  const launcher = path.join(packageRoot, packageJson.bin['chatos-document-mcp']);
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [launcher, 'mcp'],
    env: {
      ...process.env,
      CHATOS_WORKSPACE: workspace,
      CHATOS_PLUGIN_ARTIFACT_DIR: artifact,
      CHATOS_PLUGIN_ROOT: packageRoot
    },
    stderr: 'pipe'
  });
  const client = new Client({ name: 'packed-document-mcp-smoke', version: '1.0.0' });
  await client.connect(transport);
  try {
    const listed = await client.listTools();
    if (listed.tools.length !== 15) throw new Error('packed MCP exposed an unexpected tool count');
    const created = await client.callTool({
      name: 'office_create',
      arguments: {
        format: 'docx',
        outputName: 'packed-smoke.docx',
        operations: [{ type: 'word_add_paragraph', text: 'Packed artifact smoke test' }]
      }
    });
    if (created.isError) throw new Error('packed MCP could not execute its bundled OfficeCLI');
    const candidates = created._meta?.['chatos/artifacts'];
    if (!Array.isArray(candidates) || candidates.length !== 1 || candidates[0]?.relative_path !== 'packed-smoke.docx') {
      throw new Error('packed MCP did not publish its Office Artifact registration candidate');
    }
    const metadata = await stat(path.join(artifact, 'packed-smoke.docx'));
    if (!metadata.isFile() || metadata.size < 1) throw new Error('packed MCP did not create its smoke-test artifact');
    const inspected = await client.callTool({
      name: 'document_inspect',
      arguments: { inputPath: 'packed-smoke.docx' }
    });
    if (inspected.isError || inspected.structuredContent?.source?.relativePath !== 'packed-smoke.docx') {
      throw new Error('packed MCP could not inspect a managed Artifact created in the same session');
    }
    return { tools: listed.tools.length, officeArtifactBytes: metadata.size };
  } finally {
    await client.close();
  }
}

if (packageJson.version !== manifest.version) {
  throw new Error('package.json and chatos.plugin.json versions differ');
}
if (packageJson.license !== 'Apache-2.0' || manifest.license !== packageJson.license) {
  throw new Error('package and ChatOS manifest must declare Apache-2.0');
}
if (packageJson.private === true || packageJson.publishConfig?.access !== 'public') {
  throw new Error('package must be configured for public npm publication');
}
if (packageJson.scripts?.preinstall || packageJson.scripts?.install || packageJson.scripts?.postinstall || packageJson.scripts?.prepare) {
  throw new Error('install lifecycle scripts are not allowed');
}
await execFileSync('node', ['scripts/generate-sbom.mjs', '--check'], {
  cwd: projectRoot,
  encoding: 'utf8'
});

const outputDirectory = await mkdtemp(path.join(os.tmpdir(), 'chatos-document-mcp-pack-'));
try {
  const output = execFileSync('npm', ['pack', '--json', '--pack-destination', outputDirectory], {
    cwd: projectRoot,
    encoding: 'utf8'
  });
  const [packed] = JSON.parse(output);
  if (!packed?.filename) throw new Error('npm pack did not return an artifact');
  const maxPackageBytes = 256 * 1024 * 1024;
  const maxUnpackedBytes = 768 * 1024 * 1024;
  const maxFileBytes = 128 * 1024 * 1024;
  if (packed.size > maxPackageBytes) throw new Error('packed artifact exceeds ChatOS package limit');
  if (packed.unpackedSize > maxUnpackedBytes) throw new Error('packed artifact exceeds ChatOS unpacked limit');
  if (packed.entryCount > 8192) throw new Error('packed artifact exceeds ChatOS entry limit');
  for (const file of packed.files ?? []) {
    if (file.size > maxFileBytes) throw new Error(`packed file exceeds ChatOS limit: ${file.path}`);
  }
  const packedPaths = new Set((packed.files ?? []).map((file) => file.path));
  const requiredComplianceFiles = [
    'SBOM.cdx.json',
    'LICENSE',
    'NOTICE',
    'PDFIUM_THIRD_PARTY_NOTICES.txt',
    'THIRD_PARTY_LICENSES.txt',
    'THIRD_PARTY_NOTICES.md',
    'vendor/officecli-v1.0.144.json',
    'vendor/pdfium-v7243.json',
    `vendor/${pdfiumManifest.review.thirdPartyManifest}`,
    'vendor/licenses/OfficeCLI-LICENSE.txt',
    'vendor/licenses/PDFium-BSD-3-Clause.txt',
    'vendor/licenses/hyzyla-pdfium-MIT.txt',
    'vendor/licenses/pdfium-lib-MIT.txt',
    ...pdfiumThirdPartyManifest.components.flatMap((component) =>
      component.licenseAssets.map((asset) => `vendor/${asset.target}`)
    )
  ];
  for (const required of new Set(requiredComplianceFiles)) {
    if (!packedPaths.has(required)) throw new Error(`packed artifact is missing required compliance file: ${required}`);
  }
  const extractionRoot = path.join(outputDirectory, 'extracted');
  await mkdir(extractionRoot);
  await extractTarball(path.join(outputDirectory, packed.filename), extractionRoot);
  const smoke = await smokeTestPackedPackage(path.join(extractionRoot, 'package'), outputDirectory);
  process.stdout.write(`${JSON.stringify({
    filename: packed.filename,
    size: packed.size,
    unpackedSize: packed.unpackedSize,
    entryCount: packed.entryCount,
    shasum: packed.shasum,
    integrity: packed.integrity,
    smoke
  }, null, 2)}\n`);
} finally {
  await rm(outputDirectory, { recursive: true, force: true });
}

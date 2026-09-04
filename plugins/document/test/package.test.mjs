import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { access, readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

const projectRoot = path.resolve(import.meta.dirname, '..');

async function probeMcpStdout() {
  const child = spawn(process.execPath, [path.join(projectRoot, 'bin', 'chatos-document-mcp'), 'mcp'], {
    env: {
      ...process.env,
      CHATOS_WORKSPACE: os.tmpdir(),
      CHATOS_PLUGIN_ARTIFACT_DIR: os.tmpdir(),
      CHATOS_PLUGIN_ROOT: projectRoot
    },
    stdio: ['pipe', 'pipe', 'pipe']
  });
  let stdout = '';
  let stderr = '';
  child.stdout.setEncoding('utf8');
  child.stderr.setEncoding('utf8');
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  child.stdin.end(`${JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'initialize',
    params: {
      protocolVersion: '2025-03-26',
      capabilities: {},
      clientInfo: { name: 'stdout-probe', version: '1.0.0' }
    }
  })}\n`);
  const exitCode = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', resolve);
  });
  return { exitCode, stdout, stderr };
}

test('package and ChatOS manifest identities stay aligned', async () => {
  const packageJson = JSON.parse(await readFile(path.join(projectRoot, 'package.json'), 'utf8'));
  const manifest = JSON.parse(await readFile(path.join(projectRoot, 'chatos.plugin.json'), 'utf8'));
  assert.equal(packageJson.version, manifest.version);
  assert.equal(packageJson.license, 'Apache-2.0');
  assert.equal(manifest.license, packageJson.license);
  assert.equal(packageJson.private, undefined);
  assert.equal(packageJson.publishConfig.access, 'public');
  assert.equal(packageJson.bin['chatos-document-mcp'], 'bin/chatos-document-mcp');
  assert.equal(manifest.mcpServers['document-mcp'].bin, 'chatos-document-mcp');
  assert.ok(manifest.permissions.some((permission) => permission.permission === 'artifact.create'));
  for (const lifecycle of ['preinstall', 'install', 'postinstall', 'prepare']) {
    assert.equal(packageJson.scripts?.[lifecycle], undefined);
  }
  await access(path.join(projectRoot, 'dist', 'server.mjs'));
  await access(path.join(projectRoot, 'dist', 'pdfium.wasm'));
  await access(path.join(projectRoot, 'SBOM.cdx.json'));
  await access(path.join(projectRoot, 'PDFIUM_THIRD_PARTY_NOTICES.txt'));
  await access(path.join(projectRoot, 'THIRD_PARTY_LICENSES.txt'));
  await access(path.join(projectRoot, 'LICENSE'));
  await access(path.join(projectRoot, 'NOTICE'));
  const sbom = JSON.parse(await readFile(path.join(projectRoot, 'SBOM.cdx.json'), 'utf8'));
  assert.equal(sbom.bomFormat, 'CycloneDX');
  assert.equal(sbom.specVersion, '1.5');
  assert.equal(sbom.metadata.component.name, packageJson.name);
  assert.equal(sbom.metadata.component.version, packageJson.version);
  assert.ok(sbom.components.some((component) => component.name === 'OfficeCLI'));
  assert.ok(sbom.components.some((component) => component.name === 'PDFium WebAssembly'));
  for (const name of ['FreeType', 'ICU', 'OpenJPEG', 'libjpeg-turbo', 'Emscripten runtime and generated glue']) {
    assert.ok(sbom.components.some((component) => component.name === name), `missing SBOM component: ${name}`);
  }
  const pdfiumManifest = JSON.parse(await readFile(path.join(projectRoot, 'vendor', 'pdfium-v7243.json'), 'utf8'));
  assert.equal(pdfiumManifest.review.status, 'complete');
  const pdfiumThirdParty = JSON.parse(await readFile(
    path.join(projectRoot, 'vendor', pdfiumManifest.review.thirdPartyManifest),
    'utf8'
  ));
  assert.equal(pdfiumThirdParty.subject.wasmSha256, pdfiumManifest.asset.sha256);
  assert.equal(pdfiumThirdParty.components.length, 16);
});

test('stdio MCP reserves stdout for JSON-RPC messages', async () => {
  const result = await probeMcpStdout();
  assert.equal(result.exitCode, 0, result.stderr);
  const lines = result.stdout.trim().split('\n');
  assert.equal(lines.length, 1, result.stdout);
  const response = JSON.parse(lines[0]);
  assert.equal(response.jsonrpc, '2.0');
  assert.equal(response.id, 1);
  assert.equal(response.result.serverInfo.name, 'chatos-document-mcp');
  assert.doesNotMatch(result.stdout, /Warning:/);
});

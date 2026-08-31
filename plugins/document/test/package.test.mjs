import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

const projectRoot = path.resolve(import.meta.dirname, '..');

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

'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const packageRoot = path.join(__dirname, '..');
const packageJson = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot, 'chatos.plugin.json'), 'utf8'));
const sbom = JSON.parse(fs.readFileSync(path.join(packageRoot, 'sbom.cdx.json'), 'utf8'));

test('package and manifest versions are identical', () => {
  assert.equal(packageJson.version, manifest.version);
});

test('package has no install lifecycle scripts', () => {
  for (const name of ['preinstall', 'install', 'postinstall', 'prepare']) {
    assert.equal(packageJson.scripts?.[name], undefined);
  }
});

test('manifest launches the installed stdio binary', () => {
  assert.deepEqual(manifest.mcpServers['browser-cdp'], {
    type: 'stdio',
    bin: 'chatos-browser-cdp',
    requiresExclusiveExecution: true,
    args: ['mcp'],
    env: {}
  });
});

test('CycloneDX SBOM contains the resolved Rust dependency graph', () => {
  assert.equal(sbom.bomFormat, 'CycloneDX');
  assert.equal(sbom.specVersion, '1.5');
  assert.ok(Array.isArray(sbom.components));
  assert.ok(sbom.components.length > 100);
  assert.ok(sbom.components.some(component => component.name === 'browser-cdp-cli'));
  assert.ok(sbom.components.some(component => component.name === 'chromiumoxide'));
});

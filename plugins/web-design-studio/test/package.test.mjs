import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import test from 'node:test';

test('package and plugin manifests stay version-aligned with built UI assets', async () => {
  const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
  const chatosManifest = JSON.parse(await readFile('chatos.plugin.json', 'utf8'));
  const codexManifest = JSON.parse(await readFile('.codex-plugin/plugin.json', 'utf8'));

  assert.equal(packageJson.version, chatosManifest.version);
  assert.equal(packageJson.version, codexManifest.version);
  assert.equal(packageJson.bin['chatos-web-design-studio'], 'bin/chatos-web-design-studio');
  assert.equal(chatosManifest.mcpServers['web-design-mcp'].bin, 'chatos-web-design-studio');
  assert.equal(chatosManifest.ui[0].runtime.bin, 'chatos-web-design-studio');
  assert.equal(codexManifest.name, 'web-design-studio');

  await Promise.all([
    access('ui/index.html'),
    access('ui/assets/app.js'),
    access('ui/assets/app.css'),
    access('dist/mcp-server.mjs'),
    access('dist/studio-server.mjs')
  ]);
});

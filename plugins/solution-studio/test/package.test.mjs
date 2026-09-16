import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import test from 'node:test';

test('package contains manifests, built UI, runtime entries, and finished skills', async () => {
  for (const file of ['.codex-plugin/plugin.json', '.mcp.json', 'chatos.plugin.json', 'ui/index.html', 'ui/assets/app.js', 'ui/assets/app.css', 'dist/mcp-server.mjs', 'dist/studio-server.mjs', 'dist/runtime-context.mjs']) await access(file);
  const manifest = JSON.parse(await readFile('.codex-plugin/plugin.json', 'utf8'));
  assert.equal(manifest.name, 'solution-studio');
  assert.equal(manifest.mcpServers, './.mcp.json');
  const chatosManifest = JSON.parse(await readFile('chatos.plugin.json', 'utf8'));
  assert.deepEqual(chatosManifest.runtimeContext.optional, ['project.id', 'workspace.id', 'workspace.root']);
  const files = ['skills/solution-studio/SKILL.md', 'skills/solution-discovery/SKILL.md', 'skills/solution-design/SKILL.md', 'skills/solution-execution-plan/SKILL.md', 'skills/solution-validation/SKILL.md'];
  for (const file of files) assert.doesNotMatch(await readFile(file, 'utf8'), /TODO|\[TODO:/);
  const app = await readFile('ui/assets/app.js', 'utf8');
  assert.doesNotMatch(app, /Diagram Studio|关联图表/);
});

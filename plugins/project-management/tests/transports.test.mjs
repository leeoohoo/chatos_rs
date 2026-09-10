import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { get } from 'node:http';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { PlanningStore } from '../dist/store.mjs';
import { startPlanningServer } from '../dist/http.mjs';

test('HTTP UI and stdio MCP share business data and enforce the transport boundaries', async t => {
  const dir = mkdtempSync(path.join(tmpdir(), 'chatos-planning-transport-'));
  const env = { CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'client-project', CHATOS_PROJECT_NAME: '<script>project</script>', CHATOS_CONTEXT_SCOPE_ID: 'a'.repeat(64), CHATOS_PLUGIN_DATA_DIR: dir };
  const store = new PlanningStore(env);
  const { server, origin } = await startPlanningServer(store, 0);
  const client = new Client({ name: 'planning-tests', version: '1.0.0' });
  const transport = new StdioClientTransport({ command: process.execPath, args: ['bin/chatos-project-management', 'mcp'], env, stderr: 'pipe' });
  t.after(async () => { await client.close(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); store.close(); rmSync(dir, { recursive: true }); });
  await client.connect(transport);
  const tools = await client.listTools();
  assert.deepEqual(tools.tools.map(v => v.name), ['planning_read', 'planning_get', 'planning_scope', 'planning_change']);
  assert.equal(JSON.stringify(tools).includes('project.create'), false);
  assert.equal((await fetch(origin + '/api/state')).status, 403);
  assert.equal((await fetch(origin + '/', { headers: { Origin: 'https://attacker.example' } })).status, 403);
  const reboundStatus = await new Promise((resolve, reject) => {
    get(origin + '/', { headers: { Host: 'attacker.example' } }, response => { response.resume(); resolve(response.statusCode); }).on('error', reject);
  });
  assert.equal(reboundStatus, 403);
  const page = await fetch(origin + '/');
  assert.match(page.headers.get('content-security-policy'), /default-src 'none'/);
  const html = await page.text();
  assert.equal(html.includes('<script>project</script>'), false);
  const token = html.match(/name="planning-session" content="([a-f0-9]+)"/)[1];
  const headers = { 'x-planning-session': token, 'Content-Type': 'application/json', Origin: origin };
  const command = { operation: 'requirement.create', requestId: randomUUID(), expectedRevision: 0, title: 'HTTP requirement', detail: 'shared data', acceptanceCriteria: 'criteria', parentId: null, status: 'draft' };
  assert.equal((await fetch(origin + '/api/changes', { method: 'POST', headers: { ...headers, Origin: 'https://attacker.example' }, body: JSON.stringify(command) })).status, 403);
  const created = await fetch(origin + '/api/changes', { method: 'POST', headers, body: JSON.stringify(command) });
  assert.equal(created.status, 200); const saved = await created.json();
  const read = await client.callTool({ name: 'planning_read', arguments: {} });
  const index = JSON.parse(read.content[0].text);
  assert.equal(index.requirements[0].id, saved.result.id); assert.equal(index.requirements[0].detail, undefined);
  const write = await client.callTool({ name: 'planning_change', arguments: {
    operation: 'document.create', title: 'Shared technical doc', kind: 'technical', format: 'markdown', status: 'published',
    requirementIds: [saved.result.id], content: '# Shared technical doc', expectedRevision: 1, requestId: randomUUID()
  } });
  assert.equal(write.isError, undefined);
  const state = await (await fetch(origin + '/api/state', { headers })).json();
  assert.equal(state.documents[0].title, 'Shared technical doc');
  assert.equal(state.documentVersions[0].content, '# Shared technical doc');
  assert.deepEqual(state.documentLinks.map(value => ({ ...value })), [{ documentId: state.documents[0].id, requirementId: saved.result.id }]);
  assert.equal(state.context.projectName, '<script>project</script>');
  assert.equal(state.context.dataDir, undefined);
  const conflict = await fetch(origin + '/api/changes', { method: 'POST', headers, body: JSON.stringify({ ...command, requestId: randomUUID() }) });
  assert.equal(conflict.status, 409);
  const injection = await client.callTool({ name: 'planning_change', arguments: { ...command, project_id: 'injected' } });
  assert.equal(injection.isError, true);
  assert.equal((await fetch(origin + '/api/projects', { headers })).status, 404);
  assert.equal((await fetch(origin + '/planning.sqlite3', { headers })).status, 404);
  const document = await (await fetch(origin + `/api/documents/${state.documents[0].id}`, { headers })).json();
  assert.equal(document.versions[0].content, '# Shared technical doc');
  const scope = await (await fetch(origin + `/api/scope?kind=requirement&id=${saved.result.id}`, { headers })).json();
  assert.deepEqual(scope.documentIds, [state.documents[0].id]);
  const javascript = await (await fetch(origin + '/app.js')).text();
  assert.equal(javascript.includes('.innerHTML'), false);
  assert.equal(javascript.includes('allowedElements'), true);
  assert.equal(javascript.includes('eval('), false);
});

import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

test('MCP can create, patch, list, and resolve a component request', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-studio-test-'));
  const client = new Client({ name: 'web-design-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp'],
    env: { ...process.env, WEB_DESIGN_STUDIO_DATA_DIR: root, CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'project-1' }
  });
  try {
    await client.connect(transport);
    const tools = await client.listTools();
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_apply_patch'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_component_library'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_list_requests'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_auto_layout'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_export_html'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_export_react'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_export_vue'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_sync_symbol_instances'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_update_symbol_from_instance'));

    const library = await client.callTool({ name: 'web_design_get_component_library', arguments: {} });
    assert.equal(library.structuredContent.library.name, 'antd');
    assert.equal(library.structuredContent.components.length, 68);
    assert.equal(library.structuredContent.themes.length, 6);
    assert.equal(library.structuredContent.components.find((component) => component.id === 'Input').variants.length, 7);

    const created = await client.callTool({ name: 'web_design_create_document', arguments: { title: 'MCP Website' } });
    const documentId = created.structuredContent.document.documentId;
    const read = await client.callTool({ name: 'web_design_get_document', arguments: { documentId } });
    assert.equal(read.structuredContent.document.revision, 1);

    const patched = await client.callTool({
      name: 'web_design_apply_patch',
      arguments: {
        documentId,
        expectedRevision: 1,
        operations: [
          { op: 'update_component', componentId: 'hero-heading', changes: { content: '由 AI 修改的标题' } },
          { op: 'set_parent', componentId: 'hero-heading', parentId: 'hero-section' },
          { op: 'set_parent', componentId: 'hero-copy', parentId: 'hero-section' },
          { op: 'set_layout', componentId: 'hero-section', layout: { mode: 'flex-column', gap: 12, padding: 20, align: 'start' } },
          { op: 'set_tokens', tokens: { colors: { primary: '#5D50DF', accent: '#22C55E', surface: '#FFFFFF', text: '#17132E', muted: '#6B7280' }, radii: { small: 8, medium: 16, large: 28 }, typography: { fontFamily: 'Inter, sans-serif', baseFontSize: 16 } } },
          { op: 'set_breakpoint', device: 'mobile', width: 430, height: 1500 },
          { op: 'move_component', componentId: 'hero-heading', device: 'mobile', x: 48, y: 80 },
          { op: 'update_component', componentId: 'hero-heading', device: 'mobile', changes: { style: { fontSize: 35 } } },
          {
            op: 'add_request',
            request: {
              id: 'request-mcp',
              componentId: 'hero-primary-action',
              instruction: '把按钮改为绿色',
              status: 'pending',
              createdAt: new Date().toISOString()
            }
          }
        ]
      }
    });
    assert.equal(patched.structuredContent.document.revision, 2);
    assert.equal(patched.structuredContent.document.breakpoints.mobile.width, 430);
    assert.equal(patched.structuredContent.document.components.find((component) => component.id === 'hero-heading').responsive.mobile.x, 48);

    const laidOut = await client.callTool({
      name: 'web_design_auto_layout',
      arguments: { documentId, expectedRevision: 2, containerId: 'hero-section', device: 'mobile' }
    });
    assert.equal(laidOut.structuredContent.document.revision, 3);
    assert.equal(laidOut.structuredContent.document.components.find((component) => component.id === 'hero-heading').responsive.mobile.x, 36);

    const requests = await client.callTool({ name: 'web_design_list_requests', arguments: { documentId } });
    assert.equal(requests.structuredContent.requests.length, 1);
    assert.equal(requests.structuredContent.requests[0].component.id, 'hero-primary-action');

    const exported = await client.callTool({ name: 'web_design_export_html', arguments: { documentId, pageId: 'home', device: 'mobile' } });
    assert.equal(exported.structuredContent.files[0].filename, 'index.html');
    assert.match(exported.structuredContent.files[0].html, /width: 430px/);

    const reactExport = await client.callTool({ name: 'web_design_export_react', arguments: { documentId, device: 'mobile' } });
    assert.equal(reactExport.structuredContent.files[0].filename, 'WebDesignApp.jsx');
    assert.match(reactExport.structuredContent.files[0].content, /window\.history\.pushState/);

    const vueExport = await client.callTool({ name: 'web_design_export_vue', arguments: { documentId, device: 'mobile' } });
    assert.equal(vueExport.structuredContent.files[0].filename, 'WebDesignApp.vue');
    assert.match(vueExport.structuredContent.files[0].content, /<script setup>/);

    const resolved = await client.callTool({
      name: 'web_design_resolve_request',
      arguments: { documentId, expectedRevision: 3, requestId: 'request-mcp', resolution: '按钮已更新' }
    });
    assert.equal(resolved.structuredContent.document.revision, 4);
    assert.equal(resolved.structuredContent.document.requests[0].status, 'resolved');
  } finally {
    await client.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
  }
});

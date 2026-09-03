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
    env: {
      ...process.env,
      WEB_DESIGN_STUDIO_DATA_DIR: root,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'host-project-through-123',
      CHATOS_PROJECT_NAME: '宿主产品项目',
      CHATOS_WORKSPACE_ID: 'workspace-through-456'
    }
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
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_list_projects'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_create_project'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_project'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_move_document'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_insert_section'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_apply_page_template'));
    assert.equal(tools.tools.some((tool) => Object.hasOwn(tool.inputSchema.properties ?? {}, 'chatosProjectId')), false);

    const projectList = await client.callTool({ name: 'web_design_list_projects', arguments: {} });
    assert.equal(projectList.structuredContent.scope.chatosProjectId, 'host-project-through-123');
    assert.equal(projectList.structuredContent.scope.chatosProjectName, '宿主产品项目');
    assert.equal(projectList.structuredContent.scope.workspaceId, 'workspace-through-456');

    const createdProject = await client.callTool({ name: 'web_design_create_project', arguments: { name: 'Web Studio 内部项目' } });
    const internalProjectId = createdProject.structuredContent.project.projectId;
    assert.notEqual(internalProjectId, projectList.structuredContent.scope.chatosProjectId);
    assert.equal(createdProject.structuredContent.scope.chatosProjectId, 'host-project-through-123');

    const library = await client.callTool({ name: 'web_design_get_component_library', arguments: {} });
    assert.deepEqual(library.structuredContent.libraries.map((item) => item.id), ['antd', 'chakra', 'shadcn']);
    assert.equal(library.structuredContent.libraries.find((item) => item.id === 'antd').components.length, 72);
    const chakraLibrary = library.structuredContent.libraries.find((item) => item.id === 'chakra');
    assert.equal(chakraLibrary.components.length, 114);
    assert.equal(chakraLibrary.components.every((component) => component.variants.length >= 2), true);
    assert.ok(library.structuredContent.libraries.find((item) => item.id === 'shadcn').components.length >= 45);
    assert.equal(library.structuredContent.themes.length, 6);
    assert.equal(library.structuredContent.sections.length, 28);
    assert.equal(library.structuredContent.pageTemplates.length, 8);
    assert.equal(library.structuredContent.sections.some((section) => section.id === 'hero-centered'), true);
    assert.equal(library.structuredContent.pageTemplates.some((template) => template.id === 'developer'), true);
    assert.equal(library.structuredContent.libraries.find((item) => item.id === 'antd').components.find((component) => component.id === 'Input').variants.length, 9);

    const templateSeed = await client.callTool({ name: 'web_design_create_document', arguments: { projectId: internalProjectId, title: 'Template Website', blank: true } });
    const templateDocumentId = templateSeed.structuredContent.document.documentId;
    const templated = await client.callTool({
      name: 'web_design_apply_page_template',
      arguments: { documentId: templateDocumentId, expectedRevision: 1, pageId: 'home', templateId: 'developer' }
    });
    assert.equal(templated.structuredContent.document.revision, 2);
    assert.ok(templated.structuredContent.document.components.length > 50);
    const withSection = await client.callTool({
      name: 'web_design_insert_section',
      arguments: { documentId: templateDocumentId, expectedRevision: 2, pageId: 'home', sectionId: 'gallery' }
    });
    assert.equal(withSection.structuredContent.document.revision, 3);
    assert.ok(withSection.structuredContent.document.components.length > templated.structuredContent.document.components.length);

    const created = await client.callTool({ name: 'web_design_create_document', arguments: { projectId: internalProjectId, title: 'MCP Website' } });
    const documentId = created.structuredContent.document.documentId;
    assert.equal(created.structuredContent.scope.chatosProjectId, 'host-project-through-123');
    const internalProject = await client.callTool({ name: 'web_design_get_project', arguments: { projectId: internalProjectId } });
    assert.deepEqual(internalProject.structuredContent.project.designIds, [templateDocumentId, documentId]);
    assert.equal(internalProject.structuredContent.scope.chatosProjectId, 'host-project-through-123');
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

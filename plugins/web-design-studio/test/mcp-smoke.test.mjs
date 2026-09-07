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
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_catalog'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_search_components'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_component_contract'));
    assert.equal(tools.tools.some((tool) => tool.name === 'web_design_get_component_library'), false);
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
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_document_outline'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_page'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_get_node'));
    assert.ok(tools.tools.some((tool) => tool.name === 'web_design_apply_node_batch'));
    assert.equal(tools.tools.some((tool) => Object.hasOwn(tool.inputSchema.properties ?? {}, 'chatosProjectId')), false);
    for (const tool of tools.tools) {
      assert.ok(tool._meta['chatos/skillGate'].allOf.length >= 2);
      assert.equal(Object.hasOwn(tool.inputSchema.properties ?? {}, 'skillEvidence'), false);
      assert.equal(tool.inputSchema.required?.includes('skillEvidence') ?? false, false);
    }
    const patchSchema = tools.tools.find((tool) => tool.name === 'web_design_apply_patch').inputSchema.properties.operations.items;
    assert.equal(tools.tools.find((tool) => tool.name === 'web_design_apply_patch').inputSchema.properties.operations.maxItems, 48);
    assert.equal(tools.tools.find((tool) => tool.name === 'web_design_apply_node_batch').inputSchema.properties.components.maxItems, 24);
    assert.ok(Array.isArray(patchSchema.oneOf));
    assert.equal(patchSchema.oneOf.some((branch) => branch.properties?.op?.const === 'move_component' && branch.required.includes('x') && branch.required.includes('y')), true);
    assert.equal(patchSchema.oneOf.some((branch) => branch.properties?.op?.const === 'update_component' && branch.required.includes('changes')), true);

    const projectList = await client.callTool({ name: 'web_design_list_projects', arguments: {} });
    assert.deepEqual(projectList.structuredContent.scope, {
      kind: 'project',
      isolated: true,
      hasProjectContext: true,
      projectName: '宿主产品项目'
    });
    assert.equal(Object.hasOwn(projectList.structuredContent.scope, 'chatosProjectId'), false);
    assert.equal(Object.hasOwn(projectList.structuredContent.scope, 'workspaceId'), false);

    const createdProject = await client.callTool({ name: 'web_design_create_project', arguments: { name: 'Web Studio 内部项目' } });
    const internalProjectId = createdProject.structuredContent.project.projectId;
    assert.match(internalProjectId, /^project-/);
    assert.equal(Object.hasOwn(createdProject.structuredContent.scope, 'chatosProjectId'), false);

    const catalog = await client.callTool({ name: 'web_design_get_catalog', arguments: {} });
    assert.deepEqual(catalog.structuredContent.libraries.map((item) => item.id), ['antd', 'chakra', 'shadcn', 'magicui', 'spell', 'inspira', 'daisyui']);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'antd').componentCount, 72);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'antd').variantCount, 828);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'chakra').componentCount, 113);
    assert.ok(catalog.structuredContent.libraries.find((item) => item.id === 'shadcn').componentCount >= 45);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'magicui').componentCount, 68);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'spell').componentCount, 33);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'inspira').componentCount, 155);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'daisyui').variantCount, 587);
    assert.equal(catalog.structuredContent.themes.length, 6);
    assert.equal(catalog.structuredContent.sections.length, 28);
    assert.equal(catalog.structuredContent.pageTemplates.length, 8);
    assert.equal(catalog.structuredContent.sections.some((section) => section.id === 'hero-centered'), true);
    assert.equal(catalog.structuredContent.pageTemplates.some((template) => template.id === 'developer'), true);

    const search = await client.callTool({
      name: 'web_design_search_components',
      arguments: { libraryId: 'antd', query: 'input', limit: 5 }
    });
    assert.ok(search.structuredContent.count >= 1);
    assert.ok(search.structuredContent.count <= 5);
    assert.equal(search.structuredContent.candidates.every((item) => item.libraryId === 'antd'), true);
    assert.equal(search.structuredContent.candidates.some((item) => item.componentId === 'Input'), true);
    assert.equal(search.structuredContent.candidates.some((item) => item.componentId === 'List'), false);

    const inputContract = await client.callTool({
      name: 'web_design_get_component_contract',
      arguments: { libraryId: 'antd', componentId: 'Input' }
    });
    assert.equal(inputContract.structuredContent.library.id, 'antd');
    assert.equal(inputContract.structuredContent.component.id, 'Input');
    assert.equal(inputContract.structuredContent.component.variants.length, 18);
    assert.equal(inputContract.structuredContent.component.bindingTemplate.name, 'antd');
    assert.equal(inputContract.structuredContent.component.bindingTemplate.component, 'Input');

    const templateSeed = await client.callTool({ name: 'web_design_create_document', arguments: { projectId: internalProjectId, title: 'Template Website', blank: true } });
    const templateDocumentId = templateSeed.structuredContent.document.documentId;
    const templated = await client.callTool({
      name: 'web_design_apply_page_template',
      arguments: { documentId: templateDocumentId, expectedRevision: 1, pageId: 'home', templateId: 'developer' }
    });
    assert.equal(templated.structuredContent.document.revision, 2);
    assert.ok(templated.structuredContent.document.componentCount > 50);
    assert.equal(Object.hasOwn(templated.structuredContent.document, 'components'), false);
    const withSection = await client.callTool({
      name: 'web_design_insert_section',
      arguments: { documentId: templateDocumentId, expectedRevision: 2, pageId: 'home', sectionId: 'gallery' }
    });
    assert.equal(withSection.structuredContent.document.revision, 3);
    assert.ok(withSection.structuredContent.document.componentCount > templated.structuredContent.document.componentCount);

    const created = await client.callTool({ name: 'web_design_create_document', arguments: { projectId: internalProjectId, title: 'MCP Website' } });
    const documentId = created.structuredContent.document.documentId;
    assert.equal(Object.hasOwn(created.structuredContent.scope, 'chatosProjectId'), false);
    const internalProject = await client.callTool({ name: 'web_design_get_project', arguments: { projectId: internalProjectId } });
    assert.deepEqual(internalProject.structuredContent.project.designIds, [templateDocumentId, documentId]);
    assert.equal(Object.hasOwn(internalProject.structuredContent.scope, 'chatosProjectId'), false);
    const read = await client.callTool({ name: 'web_design_get_document', arguments: { documentId } });
    assert.equal(read.structuredContent.document.revision, 1);
    const outline = await client.callTool({ name: 'web_design_get_document_outline', arguments: { documentId } });
    assert.equal(outline.structuredContent.pages.length, 1);
    assert.equal(Object.hasOwn(outline.structuredContent.pages[0], 'components'), false);
    assert.ok(outline.structuredContent.pages[0].rootNodes.length > 0);
    const heroNode = await client.callTool({ name: 'web_design_get_node', arguments: { documentId, componentId: 'hero-section', depth: 2 } });
    assert.equal(heroNode.structuredContent.rootId, 'hero-section');
    assert.equal(heroNode.structuredContent.pageId, 'home');
    assert.ok(heroNode.structuredContent.nodes.some((item) => item.component.id === 'hero-heading'));

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
    assert.equal(Object.hasOwn(patched.structuredContent.document, 'components'), false);
    assert.ok(patched.structuredContent.changedComponentIds.includes('hero-heading'));
    const patchedPage = await client.callTool({ name: 'web_design_get_page', arguments: { documentId, pageId: 'home' } });
    assert.equal(patchedPage.structuredContent.breakpoints.mobile.width, 430);
    assert.equal(patchedPage.structuredContent.components.find((component) => component.id === 'hero-heading').responsive.mobile.x, 48);

    const laidOut = await client.callTool({
      name: 'web_design_auto_layout',
      arguments: { documentId, expectedRevision: 2, containerId: 'hero-section', device: 'mobile' }
    });
    assert.equal(laidOut.structuredContent.document.revision, 3);
    const laidOutPage = await client.callTool({ name: 'web_design_get_page', arguments: { documentId, pageId: 'home' } });
    assert.equal(laidOutPage.structuredContent.components.find((component) => component.id === 'hero-heading').responsive.mobile.x, 36);

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
    const resolvedRequests = await client.callTool({ name: 'web_design_list_requests', arguments: { documentId, includeResolved: true } });
    assert.equal(resolvedRequests.structuredContent.requests[0].request.status, 'resolved');

    const badSeed = await client.callTool({ name: 'web_design_create_document', arguments: { projectId: internalProjectId, title: 'Invalid Text Mockup', blank: true } });
    const badDocumentId = badSeed.structuredContent.document.documentId;
    const fakeInterface = Array.from({ length: 20 }, (_, index) => `Navigation ${index}    Button ${index}    Card ${index}`).join('\n');
    const badBatch = await client.callTool({
      name: 'web_design_apply_node_batch',
      arguments: {
        documentId: badDocumentId,
        expectedRevision: 1,
        pageId: 'home',
        regionName: 'Complete dashboard',
        components: [{
          id: 'whole-page-text',
          type: 'text',
          name: '完整工作台界面',
          pageId: 'home',
          x: 20,
          y: 20,
          width: 1100,
          height: 850,
          zIndex: 1,
          content: fakeInterface,
          style: { fontSize: 14 },
          responsive: {
            tablet: { x: 20, y: 20, width: 700, height: 850 },
            mobile: { x: 20, y: 20, width: 350, height: 760 }
          },
          annotations: []
        }]
      }
    });
    assert.equal(badBatch.isError, false);
    assert.equal(badBatch.structuredContent.document.componentCount, 1);
    const draftValidation = await client.callTool({ name: 'web_design_validate', arguments: { documentId: badDocumentId, pageId: 'home', mode: 'draft' } });
    assert.equal(draftValidation.structuredContent.valid, true);
    const handoffValidation = await client.callTool({ name: 'web_design_validate', arguments: { documentId: badDocumentId, pageId: 'home', mode: 'handoff' } });
    assert.equal(handoffValidation.structuredContent.valid, false);
    assert.ok(handoffValidation.structuredContent.blockingIssues.some((issue) => issue.code === 'page_as_text_mockup'));
    assert.ok(handoffValidation.structuredContent.blockingIssues.some((issue) => issue.code === 'underbuilt_page'));
    const blockedExport = await client.callTool({ name: 'web_design_export_html', arguments: { documentId: badDocumentId, pageId: 'home' } });
    assert.equal(blockedExport.isError, true);
    assert.match(blockedExport.structuredContent.error, /not ready for export/);

    const focusedSeed = await client.callTool({ name: 'web_design_create_document', arguments: { projectId: internalProjectId, title: 'Focused batches', blank: true } });
    const focusedDocumentId = focusedSeed.structuredContent.document.documentId;
    const withSecondPage = await client.callTool({
      name: 'web_design_apply_patch',
      arguments: {
        documentId: focusedDocumentId,
        expectedRevision: 1,
        operations: [{ op: 'upsert_page', page: { id: 'settings', name: '设置', slug: '/settings' } }]
      }
    });
    assert.equal(withSecondPage.structuredContent.document.revision, 2);
    const node = (id, pageId, x = 20) => ({
      id,
      type: 'button',
      name: id,
      pageId,
      x,
      y: 20,
      width: 120,
      height: 44,
      zIndex: 1,
      content: id,
      style: {},
      annotations: []
    });
    const crossPagePatch = await client.callTool({
      name: 'web_design_apply_patch',
      arguments: {
        documentId: focusedDocumentId,
        expectedRevision: 2,
        operations: [
          { op: 'upsert_component', component: node('home-button', 'home') },
          { op: 'upsert_component', component: node('settings-button', 'settings') }
        ]
      }
    });
    assert.equal(crossPagePatch.isError, true);
    assert.match(crossPagePatch.structuredContent.error, /only one page/);
    const oversizedComponentPatch = await client.callTool({
      name: 'web_design_apply_patch',
      arguments: {
        documentId: focusedDocumentId,
        expectedRevision: 2,
        operations: Array.from({ length: 25 }, (_, index) => ({
          op: 'upsert_component',
          component: node(`button-${index}`, 'home', 20 + index * 4)
        }))
      }
    });
    assert.equal(oversizedComponentPatch.isError, true);
    assert.match(oversizedComponentPatch.structuredContent.error, /at most 24 components/);
  } finally {
    await client.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
  }
});

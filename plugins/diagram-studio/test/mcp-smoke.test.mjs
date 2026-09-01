import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

test('MCP can create, patch, layout, validate, and export a diagram', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-test-'));
  const artifacts = path.join(root, 'artifacts');
  const client = new Client({ name: 'diagram-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp'],
    env: {
      ...process.env,
      DIAGRAM_STUDIO_DATA_DIR: path.join(root, 'data'),
      CHATOS_PLUGIN_ARTIFACT_DIR: artifacts
    }
  });
  try {
    await client.connect(transport);
    const tools = await client.listTools();
    assert.ok(tools.tools.some((tool) => tool.name === 'diagram_apply_patch'));

    const created = await client.callTool({
      name: 'diagram_create_document',
      arguments: { kind: 'architecture', title: 'MCP Architecture' }
    });
    const createdBody = created.structuredContent;
    const documentId = createdBody.document.documentId;
    assert.equal(createdBody.document.revision, 1);

    const patched = await client.callTool({
      name: 'diagram_apply_patch',
      arguments: {
        documentId,
        expectedRevision: 1,
        operations: [{ op: 'set_title', title: 'Updated Architecture' }]
      }
    });
    assert.equal(patched.structuredContent.document.revision, 2);
    assert.equal(patched.structuredContent.document.title, 'Updated Architecture');

    const laidOut = await client.callTool({
      name: 'diagram_auto_layout',
      arguments: { documentId, expectedRevision: 2, direction: 'RIGHT' }
    });
    assert.equal(laidOut.structuredContent.document.revision, 3);

    const validation = await client.callTool({
      name: 'diagram_validate',
      arguments: { documentId }
    });
    assert.equal(validation.structuredContent.valid, true);

    const exported = await client.callTool({
      name: 'diagram_export',
      arguments: { documentId, format: 'svg' }
    });
    assert.equal(exported.structuredContent.mimeType, 'image/svg+xml');
    assert.match(exported.structuredContent.sha256, /^[0-9a-f]{64}$/);
  } finally {
    await client.close();
    await rm(root, { recursive: true, force: true });
  }
});

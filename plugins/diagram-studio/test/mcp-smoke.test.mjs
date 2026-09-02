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
      CHATOS_PLUGIN_ARTIFACT_DIR: artifacts,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'chatos-project-1',
      CHATOS_PROJECT_NAME: 'ChatOS Project One',
      CHATOS_WORKSPACE_ID: 'workspace-1'
    }
  });
  try {
    await client.connect(transport);
    const tools = await client.listTools();
    assert.ok(tools.tools.some((tool) => tool.name === 'diagram_apply_patch'));
    assert.ok(tools.tools.some((tool) => tool.name === 'diagram_import_plantuml'));
    assert.ok(tools.tools.some((tool) => tool.name === 'diagram_create_project'));

    const projectCreated = await client.callTool({
      name: 'diagram_create_project',
      arguments: { name: 'Architecture Project', description: 'Managed through MCP' }
    });
    const projectId = projectCreated.structuredContent.project.projectId;
    assert.equal(projectCreated.structuredContent.scope.kind, 'project');
    assert.equal(projectCreated.structuredContent.scope.chatosProjectId, 'chatos-project-1');
    const targetProjectCreated = await client.callTool({
      name: 'diagram_create_project',
      arguments: { name: 'Target Project' }
    });
    const targetProjectId = targetProjectCreated.structuredContent.project.projectId;

    const blankCreated = await client.callTool({
      name: 'diagram_create_document',
      arguments: {
        kind: 'architecture',
        title: 'Blank Architecture',
        projectId,
        blank: true
      }
    });
    const blankDocumentId = blankCreated.structuredContent.document.documentId;
    const blankDocument = await client.callTool({
      name: 'diagram_get_document',
      arguments: { documentId: blankDocumentId }
    });
    assert.deepEqual(blankDocument.structuredContent.document.nodes, []);
    assert.deepEqual(blankDocument.structuredContent.document.edges, []);

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

    const imported = await client.callTool({
      name: 'diagram_import_plantuml',
      arguments: {
        source: '@startuml\nactor User\nparticipant API\nUser -> API: Request\nAPI --> User: Response\n@enduml',
        title: 'Imported Sequence',
        projectId
      }
    });
    assert.equal(imported.structuredContent.document.kind, 'sequence');
    const plantUmlExport = await client.callTool({
      name: 'diagram_export',
      arguments: { documentId: imported.structuredContent.document.documentId, format: 'plantuml' }
    });
    assert.equal(plantUmlExport.structuredContent.mimeType, 'text/vnd.plantuml');
    assert.match(plantUmlExport.structuredContent.relativePath, /\.puml$/);

    const importedActivity = await client.callTool({
      name: 'diagram_import_plantuml',
      arguments: {
        source: '@startuml\nstart\n:Check request;\nif (Valid?) then (yes)\n:Process;\nelse (no)\n:Reject;\nendif\nstop\n@enduml',
        title: 'Imported Flow',
        kind: 'flowchart'
      }
    });
    assert.equal(importedActivity.structuredContent.document.kind, 'flowchart');
    const activityExport = await client.callTool({
      name: 'diagram_export',
      arguments: { documentId: importedActivity.structuredContent.document.documentId, format: 'plantuml' }
    });
    assert.equal(activityExport.structuredContent.mimeType, 'text/vnd.plantuml');

    const importedArchitecture = await client.callTool({
      name: 'diagram_import_plantuml',
      arguments: {
        source: '@startuml\nactor User\ncomponent "Web App" as web\ndatabase DB\nUser --> web : HTTPS\nweb --> DB : SQL\n@enduml',
        title: 'Imported Architecture'
      }
    });
    assert.equal(importedArchitecture.structuredContent.document.kind, 'architecture');
    assert.equal(importedArchitecture.structuredContent.document.notation.dialect, 'component');

    const importedTopology = await client.callTool({
      name: 'diagram_import_plantuml',
      arguments: {
        source: '@startuml\ncloud Internet\nnode "App Node" as app\ndatabase DB\nInternet --> app : HTTPS\napp --> DB : SQL\n@enduml',
        title: 'Imported Topology'
      }
    });
    assert.equal(importedTopology.structuredContent.document.kind, 'topology');
    assert.equal(importedTopology.structuredContent.document.notation.dialect, 'deployment');

    const projectDocuments = await client.callTool({
      name: 'diagram_list_documents',
      arguments: { projectId }
    });
    assert.equal(projectDocuments.structuredContent.documents.length, 2);

    await client.callTool({
      name: 'diagram_move_document',
      arguments: {
        documentId: blankDocumentId,
        sourceProjectId: projectId,
        targetProjectId
      }
    });
    const sourceAfterMove = await client.callTool({
      name: 'diagram_get_project',
      arguments: { projectId }
    });
    const targetAfterMove = await client.callTool({
      name: 'diagram_get_project',
      arguments: { projectId: targetProjectId }
    });
    assert.deepEqual(sourceAfterMove.structuredContent.project.diagramIds, [
      imported.structuredContent.document.documentId
    ]);
    assert.deepEqual(targetAfterMove.structuredContent.project.diagramIds, [blankDocumentId]);

    const renamed = await client.callTool({
      name: 'diagram_update_project',
      arguments: { projectId: targetProjectId, name: 'Renamed Target' }
    });
    assert.equal(renamed.structuredContent.project.name, 'Renamed Target');

    await client.callTool({
      name: 'diagram_delete_document',
      arguments: { documentId: imported.structuredContent.document.documentId }
    });
    const sourceAfterDelete = await client.callTool({
      name: 'diagram_get_project',
      arguments: { projectId }
    });
    assert.deepEqual(sourceAfterDelete.structuredContent.project.diagramIds, []);

    await client.callTool({
      name: 'diagram_delete_project',
      arguments: { projectId: targetProjectId, deleteDocuments: false }
    });
    const retainedDocument = await client.callTool({
      name: 'diagram_get_document',
      arguments: { documentId: blankDocumentId }
    });
    assert.equal(retainedDocument.structuredContent.document.documentId, blankDocumentId);
    await client.callTool({
      name: 'diagram_delete_document',
      arguments: { documentId: blankDocumentId }
    });
    await client.callTool({
      name: 'diagram_delete_project',
      arguments: { projectId, deleteDocuments: true }
    });
  } finally {
    await client.close();
    await rm(root, { recursive: true, force: true });
  }
});

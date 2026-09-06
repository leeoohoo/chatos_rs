import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const checklistByKind = {
  architecture: ['single_architecture_viewpoint', 'boundaries_show_ownership', 'primary_path_is_visible', 'implementation_detail_is_excluded', 'independent_concerns_are_split', 'code_evidence_is_mapped'],
  flowchart: ['single_business_outcome', 'start_and_terminal_states_are_clear', 'decisions_have_named_outcomes', 'failure_and_retry_paths_are_bounded', 'independent_processes_are_split', 'code_evidence_is_mapped'],
  swimlane: ['single_collaboration_scenario', 'lanes_represent_real_ownership', 'handoffs_are_explicit', 'decisions_have_named_outcomes', 'independent_scenarios_are_split', 'code_evidence_is_mapped'],
  topology: ['single_environment_or_traffic_question', 'deployment_boundaries_are_real', 'traffic_direction_is_visible', 'redundancy_is_not_fake_detail', 'logical_architecture_is_separated', 'configuration_evidence_is_mapped'],
  sequence: ['single_runtime_scenario', 'participants_have_distinct_roles', 'message_order_is_causal', 'activation_intervals_are_bounded', 'fragments_do_not_hide_content', 'independent_scenarios_are_split']
};

test('MCP enforces Skill-gated permits, injected scope, and idempotent generated diagrams', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-test-'));
  const client = new Client({ name: 'diagram-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp'],
    env: {
      ...process.env,
      DIAGRAM_STUDIO_DATA_DIR: path.join(root, 'data'),
      CHATOS_PLUGIN_ARTIFACT_DIR: path.join(root, 'artifacts'),
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_CONTEXT_SCOPE_ID: 'scope-project-one',
      CHATOS_PROJECT_ID: 'chatos-project-1',
      CHATOS_PROJECT_NAME: 'ChatOS Project One',
      CHATOS_WORKSPACE_ID: 'workspace-1'
    }
  });
  try {
    await client.connect(transport);
    const listedTools = await client.listTools();
    const tools = new Map(listedTools.tools.map((tool) => [tool.name, tool]));
    for (const name of ['diagram_prepare_generation', 'diagram_commit_generation']) assert.ok(tools.has(name));
    assert.equal(Object.hasOwn(tools.get('diagram_prepare_generation').inputSchema.properties, 'operation'), false);
    assert.equal(Object.hasOwn(tools.get('diagram_prepare_generation').inputSchema.properties, 'documentId'), false);
    assert.equal(tools.has('diagram_get_generation_guide'), false);
    for (const name of ['diagram_list_documents', 'diagram_create_document', 'diagram_import_plantuml']) {
      assert.equal(Object.hasOwn(tools.get(name).inputSchema.properties, 'projectId'), false, `${name} must not accept projectId`);
    }
    for (const name of ['diagram_prepare_generation', 'diagram_commit_generation', 'diagram_import_plantuml']) {
      const tool = tools.get(name);
      assert.equal(tool._meta['chatos/skillGate'].evidenceArgument, 'skillEvidence');
      assert.equal(tool.inputSchema.properties.skillEvidence.type, 'array');
      assert.ok(tool.inputSchema.required.includes('skillEvidence'));
    }
    assert.ok(tools.has('diagram_create_project'), 'UI classification projects remain available');

    const blank = await call(client, 'diagram_create_document', { kind: 'architecture', title: 'Blank Architecture', artifactKey: 'blank-architecture' });
    const blankRetry = await call(client, 'diagram_create_document', { kind: 'architecture', title: 'Blank Architecture', artifactKey: 'blank-architecture' });
    assert.equal(blankRetry.document.documentId, blank.document.documentId);
    const blankDocument = await call(client, 'diagram_get_document', { documentId: blank.document.documentId });
    assert.deepEqual(blankDocument.document.nodes, []);
    assert.deepEqual(blankDocument.document.edges, []);

    const noPermit = await client.callTool({
      name: 'diagram_import_plantuml',
      arguments: {
        source: '@startuml\nactor User\nparticipant API\nUser -> API: Request\n@enduml',
        title: 'No Permit', kind: 'sequence', artifactKey: 'no-permit'
      }
    });
    assert.equal(noPermit.isError, true);

    const goal = 'Show one token refresh runtime scenario';
    const overBudget = await client.callTool({
      name: 'diagram_prepare_generation',
      arguments: {
        skillEvidence: ['router-evidence', 'sequence-evidence'], kind: 'sequence',
        artifactKey: 'token-refresh', title: 'Token Refresh Sequence',
        plan: planFor('sequence', goal, { estimatedPrimaryItemCount: 9 })
      }
    });
    assert.equal(overBudget.isError, true);
    assert.match(overBudget.structuredContent.error, /exceeding.*budget|exceeds.*budget/i);

    const prepared = await call(client, 'diagram_prepare_generation', {
      skillEvidence: ['router-evidence', 'sequence-evidence'], kind: 'sequence',
      artifactKey: 'token-refresh', title: 'Token Refresh Sequence',
      plan: planFor('sequence', goal)
    });
    assert.equal(prepared.operation, 'create');
    const source = [
      '@startuml',
      'actor "User" as user',
      'participant "Client" as client',
      'participant "Auth API" as auth_api',
      'database "Session Store" as session_store',
      'user -> client : Continue',
      'client -> auth_api : Refresh token',
      'activate auth_api',
      'auth_api -> session_store : Validate session',
      'activate session_store',
      'session_store --> auth_api : Session state',
      'deactivate session_store',
      'auth_api --> client : New token',
      'deactivate auth_api',
      'client --> user : Continue',
      '@enduml'
    ].join('\n');
    const committed = await call(client, 'diagram_commit_generation', {
      skillEvidence: ['router-evidence', 'sequence-evidence'],
      generationPermit: prepared.generationPermit,
      source, title: 'Token Refresh Sequence', kind: 'sequence', artifactKey: 'token-refresh',
      idempotencyKey: 'token-refresh-write-1', responseDetail: 'document'
    });
    assert.equal(committed.created, true);
    assert.equal(committed.quality.ready, true);
    assert.equal(committed.document.kind, 'sequence');
    assert.equal(committed.document.generationProvenance.guideId, 'diagram-sequence');
    assert.equal(committed.document.generationProvenance.planHash, prepared.planHash);

    const revisionPrepared = await call(client, 'diagram_prepare_generation', {
      skillEvidence: ['router-evidence', 'sequence-evidence'], kind: 'sequence',
      artifactKey: 'token-refresh', title: 'Token Refresh Sequence',
      plan: planFor('sequence', goal)
    });
    assert.equal(revisionPrepared.operation, 'revise');
    assert.equal(revisionPrepared.documentId, committed.document.documentId);

    const retry = await call(client, 'diagram_commit_generation', {
      skillEvidence: ['router-evidence', 'sequence-evidence'],
      generationPermit: prepared.generationPermit,
      source, title: 'Token Refresh Sequence', kind: 'sequence', artifactKey: 'token-refresh',
      idempotencyKey: 'token-refresh-write-1'
    });
    assert.equal(retry.document.documentId, committed.document.documentId);
    assert.equal(retry.reused, true);

    const structuralPatchWithoutPermit = await client.callTool({
      name: 'diagram_apply_patch',
      arguments: {
        documentId: committed.document.documentId,
        expectedRevision: committed.document.revision,
        operations: [{ op: 'remove_edge', edgeId: committed.document.edges[0].id }]
      }
    });
    assert.equal(structuralPatchWithoutPermit.isError, true);
    assert.match(structuralPatchWithoutPermit.structuredContent.error, /generationPermit/);

    const renamed = await call(client, 'diagram_apply_patch', {
      documentId: committed.document.documentId,
      expectedRevision: committed.document.revision,
      operations: [{ op: 'set_title', title: 'Token Refresh Runtime Sequence' }]
    });
    assert.equal(renamed.document.title, 'Token Refresh Runtime Sequence');

    const listedDocuments = await call(client, 'diagram_list_documents', {});
    assert.equal(listedDocuments.scope.chatosProjectId, 'chatos-project-1');
    assert.equal(listedDocuments.documents.length, 2);

    const target = await call(client, 'diagram_create_project', { name: 'Reviewed Diagrams' });
    await call(client, 'diagram_move_document', { documentId: blank.document.documentId, targetProjectId: target.project.projectId });
    const targetAfterMove = await call(client, 'diagram_get_project', { projectId: target.project.projectId });
    assert.deepEqual(targetAfterMove.project.diagramIds, [blank.document.documentId]);

    const validation = await call(client, 'diagram_validate', { documentId: committed.document.documentId });
    assert.equal(validation.ready, true);
    const exported = await call(client, 'diagram_export', { documentId: committed.document.documentId, format: 'plantuml' });
    assert.equal(exported.mimeType, 'text/vnd.plantuml');
    assert.match(exported.sha256, /^[a-f0-9]{64}$/);
  } finally {
    await client.close();
    await rm(root, { recursive: true, force: true });
  }
});

function planFor(kind, goal, overrides = {}) {
  const structure = kind === 'sequence'
    ? ['User', 'Client', 'Auth API', 'Session Store']
    : kind === 'swimlane'
      ? ['Requester', 'Approver']
      : kind === 'topology'
        ? ['Public Edge', 'Application Cluster']
        : kind === 'architecture'
          ? ['Client Boundary', 'Service Boundary']
          : ['Request to completion'];
  return {
    goal,
    scope: 'Only the selected scenario and its primary path.',
    excludedDetails: ['Unrelated business journeys and implementation details.'],
    estimatedPrimaryItemCount: kind === 'sequence' ? 4 : 6,
    estimatedEdgeCount: 8,
    structure,
    splitPlan: ['Put unrelated scenarios in separately named diagrams.'],
    splitRationale: 'This diagram remains focused on one question; unrelated work is split.',
    checklistAcknowledgements: checklistByKind[kind],
    ...overrides
  };
}

async function call(client, name, args) {
  const response = await client.callTool({ name, arguments: args });
  assert.equal(response.isError, false, `${name} failed: ${response.structuredContent?.error ?? response.content?.[0]?.text}`);
  return response.structuredContent;
}

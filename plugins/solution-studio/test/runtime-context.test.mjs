import assert from 'node:assert/strict';
import test from 'node:test';
import { projectBindingFromContext, readHostRuntimeContext } from '../dist/runtime-context.mjs';

test('reads the ChatOS-owned runtime context without accepting user-supplied ids', () => {
  const context = readHostRuntimeContext({
    CHATOS_CONTEXT_SCOPE: 'project',
    CHATOS_CONTEXT_SCOPE_ID: 'scope-hash-123',
    CHATOS_PROJECT_ID: 'project-through-123',
    CHATOS_PROJECT_NAME: '宿主产品项目',
    CHATOS_WORKSPACE_ID: 'connector-workspace-456',
    CHATOS_WORKSPACE: '/workspace/product'
  });
  assert.deepEqual(context, {
    kind: 'project', isolated: true, hasProjectContext: true, sourceModeHint: 'existing-project',
    scopeId: 'scope-hash-123', projectId: 'project-through-123', projectName: '宿主产品项目',
    connectorWorkspaceId: 'connector-workspace-456', projectRoot: '/workspace/product'
  });
  assert.deepEqual(projectBindingFromContext(context), {
    projectId: 'project-through-123', projectName: '宿主产品项目',
    connectorWorkspaceId: 'connector-workspace-456', contextScopeId: 'scope-hash-123'
  });
});

test('does not claim a project binding when a manual preview only sets the scope name', () => {
  const context = readHostRuntimeContext({ CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_NAME: '预览项目' });
  assert.equal(context.hasProjectContext, false);
  assert.equal(context.sourceModeHint, 'greenfield');
  assert.equal(projectBindingFromContext(context), undefined);
});

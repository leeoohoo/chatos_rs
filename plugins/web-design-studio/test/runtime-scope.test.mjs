import assert from 'node:assert/strict';
import test from 'node:test';
import { runtimeScopeFingerprint } from '../dist/runtime-scope.test.mjs';

test('the transmitted ChatOS project id is part of a stable isolated runtime scope', () => {
  const names = ['CHATOS_CONTEXT_SCOPE', 'CHATOS_CONTEXT_SCOPE_ID', 'CHATOS_PROJECT_ID', 'CHATOS_WORKSPACE_ID', 'CHATOS_USER_ID', 'CHATOS_ACCOUNT_ID'];
  const original = Object.fromEntries(names.map((name) => [name, process.env[name]]));
  try {
    process.env.CHATOS_CONTEXT_SCOPE = 'project';
    process.env.CHATOS_WORKSPACE_ID = 'workspace-through-456';
    process.env.CHATOS_USER_ID = 'user-through-789';
    process.env.CHATOS_PROJECT_ID = 'host-project-a';
    const first = runtimeScopeFingerprint('/tmp/web-design-scope');
    const repeated = runtimeScopeFingerprint('/tmp/web-design-scope');
    process.env.CHATOS_PROJECT_ID = 'host-project-b';
    const second = runtimeScopeFingerprint('/tmp/web-design-scope');
    assert.match(first, /^[a-f0-9]{64}$/);
    assert.equal(repeated, first);
    assert.notEqual(second, first);
  } finally {
    for (const name of names) {
      if (original[name] === undefined) delete process.env[name];
      else process.env[name] = original[name];
    }
  }
});

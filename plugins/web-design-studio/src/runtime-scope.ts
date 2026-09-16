import { createHash } from 'node:crypto';
import path from 'node:path';

export function runtimeScopeFingerprint(rootDirectory: string): string {
  const scope = process.env.CHATOS_CONTEXT_SCOPE ?? 'device';
  const scopedIdentity = scope === 'project'
    ? { projectId: process.env.CHATOS_PROJECT_ID ?? '' }
    : scope === 'workspace'
      ? { workspaceId: process.env.CHATOS_WORKSPACE_ID ?? '' }
      : {};
  return createHash('sha256').update(JSON.stringify({
    scope,
    scopeId: process.env.CHATOS_CONTEXT_SCOPE_ID ?? '',
    ...scopedIdentity,
    dataDirectory: path.resolve(rootDirectory)
  })).digest('hex');
}

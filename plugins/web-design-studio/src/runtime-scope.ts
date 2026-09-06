import { createHash } from 'node:crypto';
import path from 'node:path';

export function runtimeScopeFingerprint(rootDirectory: string): string {
  return createHash('sha256').update(JSON.stringify({
    scope: process.env.CHATOS_CONTEXT_SCOPE ?? 'device',
    scopeId: process.env.CHATOS_CONTEXT_SCOPE_ID ?? '',
    projectId: process.env.CHATOS_PROJECT_ID ?? '',
    workspaceId: process.env.CHATOS_WORKSPACE_ID ?? '',
    userId: process.env.CHATOS_USER_ID ?? process.env.CHATOS_ACCOUNT_ID ?? '',
    dataDirectory: path.resolve(rootDirectory)
  })).digest('hex');
}

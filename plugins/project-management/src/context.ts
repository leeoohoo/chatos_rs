import path from 'node:path';

export interface ProjectContext { projectId: string; projectName: string; scopeId: string; dataDir: string }

export function readContext(env: NodeJS.ProcessEnv = process.env): ProjectContext {
  const projectId = env.CHATOS_PROJECT_ID;
  const scopeId = env.CHATOS_CONTEXT_SCOPE_ID;
  const dataDir = env.CHATOS_PLUGIN_DATA_DIR;
  if (env.CHATOS_CONTEXT_SCOPE !== 'project' || !projectId || projectId.trim() !== projectId ||
      projectId.length > 256 || /[\u0000-\u001f\u007f]/u.test(projectId) ||
      !scopeId || !/^[a-f0-9]{64}$/u.test(scopeId) || !dataDir || !path.isAbsolute(dataDir)) {
    throw new Error('A client-bound project and host-isolated plugin data directory are required');
  }
  return { projectId, scopeId, dataDir, projectName: env.CHATOS_PROJECT_NAME || projectId };
}

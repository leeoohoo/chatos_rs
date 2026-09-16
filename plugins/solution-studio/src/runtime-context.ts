import type { ChatosProjectBinding, SourceMode } from './schema.js';

export type HostContextKind = 'device' | 'workspace' | 'project';

export interface HostRuntimeContext {
  kind: HostContextKind;
  isolated: true;
  hasProjectContext: boolean;
  sourceModeHint: SourceMode;
  scopeId?: string;
  projectId?: string;
  projectName?: string;
  connectorWorkspaceId?: string;
  projectRoot?: string;
}

function clean(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized || undefined;
}

export function readHostRuntimeContext(environment: NodeJS.ProcessEnv = process.env): HostRuntimeContext {
  const rawKind = clean(environment.CHATOS_CONTEXT_SCOPE);
  const kind: HostContextKind = rawKind === 'project' || rawKind === 'workspace' ? rawKind : 'device';
  const projectId = clean(environment.CHATOS_PROJECT_ID);
  const hasProjectContext = kind === 'project' && Boolean(projectId);
  return {
    kind,
    isolated: true,
    hasProjectContext,
    sourceModeHint: hasProjectContext ? 'existing-project' : 'greenfield',
    ...(clean(environment.CHATOS_CONTEXT_SCOPE_ID) ? { scopeId: clean(environment.CHATOS_CONTEXT_SCOPE_ID) } : {}),
    ...(projectId ? { projectId } : {}),
    ...(clean(environment.CHATOS_PROJECT_NAME) ? { projectName: clean(environment.CHATOS_PROJECT_NAME) } : {}),
    ...(clean(environment.CHATOS_WORKSPACE_ID) ? { connectorWorkspaceId: clean(environment.CHATOS_WORKSPACE_ID) } : {}),
    ...(clean(environment.CHATOS_WORKSPACE) ? { projectRoot: clean(environment.CHATOS_WORKSPACE) } : {})
  };
}

export function projectBindingFromContext(context: HostRuntimeContext = readHostRuntimeContext()): ChatosProjectBinding | undefined {
  if (!context.hasProjectContext || !context.projectId) return undefined;
  return {
    projectId: context.projectId,
    ...(context.projectName ? { projectName: context.projectName } : {}),
    ...(context.connectorWorkspaceId ? { connectorWorkspaceId: context.connectorWorkspaceId } : {}),
    ...(context.scopeId ? { contextScopeId: context.scopeId } : {})
  };
}

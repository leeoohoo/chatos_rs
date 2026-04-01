import type { AiModelConfig } from '../../../../types';
import type { SendMessageRuntimeOptions } from '../../types';

interface SessionRuntimeLike {
  contactAgentId?: string | null;
  remoteConnectionId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  workspaceRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
}

interface RuntimeResolutionResult {
  effectiveContactAgentId: string | null;
  effectiveRemoteConnectionId: string | null;
  effectiveProjectId: string;
  effectiveProjectRoot: string | null;
  effectiveWorkspaceRoot: string | null;
  effectiveExecutionRoot: string | null;
  effectiveMcpEnabled: boolean;
  effectiveEnabledMcpIds: string[];
}

export const resolveRuntimeConfig = (
  sessionRuntime: SessionRuntimeLike | null | undefined,
  runtimeOptions: SendMessageRuntimeOptions,
): RuntimeResolutionResult => {
  const effectiveContactAgentId =
    (typeof runtimeOptions?.contactAgentId === 'string'
      ? runtimeOptions.contactAgentId.trim()
      : '')
    || sessionRuntime?.contactAgentId
    || null;
  const requestedProjectId = typeof runtimeOptions?.projectId === 'string'
    ? runtimeOptions.projectId.trim()
    : '';
  const hasRequestedRemoteConnectionId = Boolean(
    runtimeOptions
    && Object.prototype.hasOwnProperty.call(runtimeOptions, 'remoteConnectionId'),
  );
  const requestedRemoteConnectionId = typeof runtimeOptions?.remoteConnectionId === 'string'
    ? runtimeOptions.remoteConnectionId.trim()
    : '';
  const sessionRemoteConnectionId = typeof sessionRuntime?.remoteConnectionId === 'string'
    ? sessionRuntime.remoteConnectionId.trim()
    : '';
  const effectiveRemoteConnectionId = hasRequestedRemoteConnectionId
    ? (requestedRemoteConnectionId || null)
    : (sessionRemoteConnectionId || null);
  const sessionProjectId = typeof sessionRuntime?.projectId === 'string'
    ? sessionRuntime.projectId.trim()
    : '';
  const effectiveProjectId = requestedProjectId || sessionProjectId || '0';
  const requestedProjectRoot = typeof runtimeOptions?.projectRoot === 'string'
    ? runtimeOptions.projectRoot.trim()
    : '';
  const sessionProjectRoot = typeof sessionRuntime?.projectRoot === 'string'
    ? sessionRuntime.projectRoot.trim()
    : '';
  const requestedWorkspaceRoot = typeof runtimeOptions?.workspaceRoot === 'string'
    ? runtimeOptions.workspaceRoot.trim()
    : '';
  const sessionWorkspaceRoot = typeof sessionRuntime?.workspaceRoot === 'string'
    ? sessionRuntime.workspaceRoot.trim()
    : '';
  const effectiveWorkspaceRoot = requestedWorkspaceRoot || sessionWorkspaceRoot || null;
  const effectiveProjectRoot = effectiveProjectId === '0'
    ? null
    : (requestedProjectRoot || sessionProjectRoot || null);
  const effectiveExecutionRoot = effectiveWorkspaceRoot || effectiveProjectRoot;
  const effectiveMcpEnabled = typeof runtimeOptions?.mcpEnabled === 'boolean'
    ? runtimeOptions.mcpEnabled
    : (sessionRuntime?.mcpEnabled ?? true);
  const effectiveEnabledMcpIds = Array.isArray(runtimeOptions?.enabledMcpIds)
    ? runtimeOptions.enabledMcpIds
    : (sessionRuntime?.enabledMcpIds ?? []);

  return {
    effectiveContactAgentId,
    effectiveRemoteConnectionId,
    effectiveProjectId,
    effectiveProjectRoot,
    effectiveWorkspaceRoot,
    effectiveExecutionRoot,
    effectiveMcpEnabled,
    effectiveEnabledMcpIds,
  };
};

export const resolveSelectedModelOrThrow = (
  effectiveSelectedModelId: string | null | undefined,
  aiModelConfigs: AiModelConfig[],
): AiModelConfig => {
  if (!effectiveSelectedModelId) {
    throw new Error('请先选择一个模型');
  }
  const selectedModel = aiModelConfigs.find((model) => model.id === effectiveSelectedModelId);
  if (!selectedModel || !selectedModel.enabled) {
    throw new Error('选择的模型不可用');
  }
  return selectedModel;
};

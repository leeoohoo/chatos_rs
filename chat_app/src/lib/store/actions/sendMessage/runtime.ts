import type { AiModelConfig } from '../../../../types';
import type { SendMessageRuntimeOptions } from '../../types';

interface SessionRuntimeLike {
  contactAgentId?: string | null;
  remoteConnectionId?: string | null;
  selectedModelName?: string | null;
  selectedThinkingLevel?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  workspaceRoot?: string | null;
}

interface RuntimeResolutionResult {
  effectiveContactAgentId: string | null;
  effectiveRemoteConnectionId: string | null;
  effectiveModelName: string | null;
  effectiveThinkingLevel: string | null;
  effectiveProjectId: string;
  effectiveProjectRoot: string | null;
  effectiveWorkspaceRoot: string | null;
  effectiveExecutionRoot: string | null;
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
  const requestedModelName = typeof runtimeOptions?.modelName === 'string'
    ? runtimeOptions.modelName.trim()
    : '';
  const sessionModelName = typeof sessionRuntime?.selectedModelName === 'string'
    ? sessionRuntime.selectedModelName.trim()
    : '';
  const effectiveModelName = requestedModelName || sessionModelName || null;
  const requestedThinkingLevel = typeof runtimeOptions?.thinkingLevel === 'string'
    ? runtimeOptions.thinkingLevel.trim()
    : '';
  const sessionThinkingLevel = typeof sessionRuntime?.selectedThinkingLevel === 'string'
    ? sessionRuntime.selectedThinkingLevel.trim()
    : '';
  const effectiveThinkingLevel = requestedThinkingLevel || sessionThinkingLevel || null;
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

  return {
    effectiveContactAgentId,
    effectiveRemoteConnectionId,
    effectiveModelName,
    effectiveThinkingLevel,
    effectiveProjectId,
    effectiveProjectRoot,
    effectiveWorkspaceRoot,
    effectiveExecutionRoot,
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

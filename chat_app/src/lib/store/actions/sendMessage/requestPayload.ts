export const resolveModelCapabilities = (
  selectedModel: any,
  chatConfig: any,
): {
  supportsImages: boolean;
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
} => {
  const supportsImages = selectedModel?.supports_images === true;
  const supportsReasoning = selectedModel?.supports_reasoning === true || !!selectedModel?.thinking_level;
  const reasoningEnabled = supportsReasoning
    && (chatConfig?.reasoningEnabled === true || !!selectedModel?.thinking_level);
  return {
    supportsImages,
    supportsReasoning,
    reasoningEnabled,
  };
};

export const buildChatRequestLogPayload = ({
  sessionId,
  turnId,
  content,
  selectedModel,
  chatConfig,
  systemContext,
  attachments,
  reasoningEnabled,
  contactAgentId,
  remoteConnectionId,
  projectId,
  projectRoot,
  mcpEnabled,
  enabledMcpIds,
}: {
  sessionId: string;
  turnId: string;
  content: string;
  selectedModel: any;
  chatConfig: any;
  systemContext: string;
  attachments: any[];
  reasoningEnabled: boolean;
  contactAgentId: string | null;
  remoteConnectionId: string | null;
  projectId: string;
  projectRoot: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
}) => ({
  session_id: sessionId,
  turn_id: turnId,
  message: content,
  model_config: {
    model: selectedModel.model_name,
    provider: selectedModel.provider,
    base_url: selectedModel.base_url,
    api_key: selectedModel.api_key || '',
    temperature: chatConfig.temperature,
    thinking_level: selectedModel.thinking_level,
    supports_images: selectedModel.supports_images === true,
    supports_reasoning: selectedModel.supports_reasoning === true,
  },
  system_context: systemContext,
  attachments: attachments || [],
  reasoning_enabled: reasoningEnabled,
  contact_agent_id: contactAgentId,
  remote_connection_id: remoteConnectionId,
  project_id: projectId,
  project_root: projectRoot,
  mcp_enabled: mcpEnabled,
  enabled_mcp_ids: enabledMcpIds,
});

export const buildStreamChatRuntimeOptions = ({
  turnId,
  contactAgentId,
  remoteConnectionId,
  projectId,
  projectRoot,
  mcpEnabled,
  enabledMcpIds,
}: {
  turnId: string;
  contactAgentId: string | null;
  remoteConnectionId: string | null;
  projectId: string;
  projectRoot: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
}) => ({
  turnId,
  contactAgentId,
  remoteConnectionId,
  projectId,
  projectRoot,
  mcpEnabled,
  enabledMcpIds,
});

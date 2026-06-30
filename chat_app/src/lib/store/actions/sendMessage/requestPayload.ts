import type { AiModelConfig, ChatConfig } from '../../../../types';
import type {
  ApiAttachmentPayload,
  StreamChatLogPayload,
  StreamChatRuntimeOptions,
} from './types';

export const resolveModelCapabilities = (
  selectedModel: AiModelConfig,
  chatConfig: ChatConfig,
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

const compactLogText = (value: string, maxChars = 240): string => {
  const normalized = String(value || '').replace(/\s+/g, ' ').trim();
  if (normalized.length <= maxChars) {
    return normalized;
  }
  return `${normalized.slice(0, maxChars)}...`;
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
  workspaceRoot,
  planMode,
}: {
  sessionId: string;
  turnId: string;
  content: string;
  selectedModel: AiModelConfig;
  chatConfig: ChatConfig;
  systemContext: string;
  attachments: ApiAttachmentPayload[];
  reasoningEnabled: boolean;
  contactAgentId: string | null;
  remoteConnectionId: string | null;
  projectId: string;
  projectRoot: string | null;
  workspaceRoot: string | null;
  planMode: boolean;
}): StreamChatLogPayload => ({
  conversation_id: sessionId,
  turn_id: turnId,
  message_preview: compactLogText(content),
  message_chars: content.length,
  model_config: {
    id: selectedModel.id,
    model: selectedModel.model_name,
    provider: selectedModel.provider,
    base_url: selectedModel.base_url,
    temperature: chatConfig.temperature,
    thinking_level: selectedModel.thinking_level,
    supports_images: selectedModel.supports_images === true,
    supports_reasoning: selectedModel.supports_reasoning === true,
  },
  system_context_preview: compactLogText(systemContext),
  system_context_chars: systemContext.length,
  attachment_count: attachments?.length || 0,
  attachment_bytes: (attachments || []).reduce((total, attachment) => total + (attachment.size || 0), 0),
  attachments: (attachments || []).map((attachment) => ({
    name: attachment.name,
    mimeType: attachment.mimeType,
    size: attachment.size,
    type: attachment.type,
  })),
  reasoning_enabled: reasoningEnabled,
  plan_mode: planMode,
  contact_agent_id: contactAgentId,
  remote_connection_id: remoteConnectionId,
  project_id: projectId,
  project_root: projectRoot,
  workspace_root: workspaceRoot,
});

export const buildStreamChatRuntimeOptions = ({
  turnId,
  contactAgentId,
  remoteConnectionId,
  projectId,
  projectRoot,
  workspaceRoot,
  planMode,
}: {
  turnId: string;
  contactAgentId: string | null;
  remoteConnectionId: string | null;
  projectId: string;
  projectRoot: string | null;
  workspaceRoot: string | null;
  planMode: boolean;
}): StreamChatRuntimeOptions => ({
  turnId,
  contactAgentId,
  remoteConnectionId,
  projectId,
  projectRoot,
  workspaceRoot,
  planMode,
});

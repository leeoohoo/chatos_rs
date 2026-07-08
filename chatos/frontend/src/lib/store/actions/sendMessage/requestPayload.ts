// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { AiModelConfig, ChatConfig } from '../../../../types';
import { PUBLIC_PROJECT_ID } from '../../../domain/contactSessions';
import type {
  ApiAttachmentPayload,
  StreamChatLogPayload,
  StreamChatRuntimeOptions,
} from './types';

export const resolveModelCapabilities = (
  selectedModel: AiModelConfig,
  reasoningEnabledSetting: boolean,
): {
  supportsImages: boolean;
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
} => {
  const supportsImages = selectedModel?.supports_images === true;
  const supportsReasoning = selectedModel?.supports_reasoning === true || !!selectedModel?.thinking_level;
  const reasoningEnabled = supportsReasoning
    && reasoningEnabledSetting === true;
  return {
    supportsImages,
    supportsReasoning,
    reasoningEnabled,
  };
};

const hasConcreteProjectContext = (projectId: string | null | undefined): boolean => {
  const normalized = typeof projectId === 'string' ? projectId.trim() : '';
  return normalized.length > 0 && normalized !== '0' && normalized !== PUBLIC_PROJECT_ID;
};

export const resolveEffectivePlanMode = ({
  projectId,
  planModeEnabled,
}: {
  projectId: string | null | undefined;
  planModeEnabled: boolean | undefined;
}): boolean => hasConcreteProjectContext(projectId)
  && planModeEnabled === true;

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

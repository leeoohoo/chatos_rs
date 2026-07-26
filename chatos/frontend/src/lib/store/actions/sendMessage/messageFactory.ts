// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AiModelConfig,
  Message,
  PluginAgentAuditEntry,
  PluginCommandAuditEntry,
} from '../../../../types';
import {
  buildModelConfigMetadata,
  type PreviewAttachment,
} from './types';

export const createDraftUserMessage = ({
  sessionId,
  content,
  conversationTurnId,
  selectedModel,
  previewAttachments,
  pluginCommandInvocations,
  pluginAgentSelection,
  createdAt,
}: {
  sessionId: string;
  content: string;
  conversationTurnId: string;
  selectedModel: AiModelConfig;
  previewAttachments: PreviewAttachment[];
  pluginCommandInvocations?: PluginCommandAuditEntry[];
  pluginAgentSelection?: PluginAgentAuditEntry | null;
  createdAt: Date;
}): Message => ({
  id: `temp_user_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`,
  sessionId,
  role: 'user',
  content,
  status: 'completed',
  createdAt,
  metadata: {
    clientOptimistic: true,
    conversation_turn_id: conversationTurnId,
    ...(previewAttachments.length > 0 ? { attachments: previewAttachments } : {}),
    ...(pluginCommandInvocations?.length
      ? { plugin_command_invocations: pluginCommandInvocations }
      : {}),
    ...(pluginAgentSelection
      ? { plugin_agent_selection: pluginAgentSelection }
      : {}),
    model: selectedModel.model_name,
    ...buildModelConfigMetadata(selectedModel),
    task_runner_async: {
      mode: 'contact_async',
      overall_status: 'pending',
      source_turn_id: conversationTurnId,
    },
  },
});

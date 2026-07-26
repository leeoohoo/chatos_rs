// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface PluginCommandInvocationPayload {
  plugin_id: string;
  command_id: string;
  arguments?: string | null;
}

export interface PluginAgentSelectionPayload {
  plugin_id: string;
  agent_id: string;
}

export interface SendMessageRuntimeOptions {
  contactAgentId?: string | null;
  contactId?: string | null;
  remoteConnectionId?: string | null;
  modelConfigId?: string | null;
  modelName?: string | null;
  thinkingLevel?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  workspaceRoot?: string | null;
  pluginDeviceId?: string | null;
  pluginWorkspaceId?: string | null;
  selectedPluginIds?: string[];
  pluginCommandInvocations?: PluginCommandInvocationPayload[];
  pluginAgentSelection?: PluginAgentSelectionPayload | null;
}

export type SendMessageHandler = (
  content: string,
  attachments?: File[],
  runtimeOptions?: SendMessageRuntimeOptions,
) => void | Promise<void>;

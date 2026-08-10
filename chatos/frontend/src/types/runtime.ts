// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface PluginCommandInvocationPayload {
  plugin_id: string;
  command_id: string;
  arguments?: string | null;
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
}

export type SendMessageHandler = (
  content: string,
  attachments?: File[],
  runtimeOptions?: SendMessageRuntimeOptions,
) => void | Promise<void>;

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  planMode?: boolean;
}

export type SendMessageHandler = (
  content: string,
  attachments?: File[],
  runtimeOptions?: SendMessageRuntimeOptions,
) => void | Promise<void>;

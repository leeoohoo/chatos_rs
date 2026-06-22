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
}

export type SendMessageHandler = (
  content: string,
  attachments?: File[],
  runtimeOptions?: SendMessageRuntimeOptions,
) => void | Promise<void>;

export interface SendMessageRuntimeOptions {
  contactAgentId?: string | null;
  contactId?: string | null;
  remoteConnectionId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  workspaceRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
  autoCreateTask?: boolean;
  skillsEnabled?: boolean;
  selectedSkillIds?: string[];
}

export type SendMessageHandler = (
  content: string,
  attachments?: File[],
  runtimeOptions?: SendMessageRuntimeOptions,
) => void | Promise<void>;

export type GuideMessageHandler = (
  content: string,
  attachments?: File[],
) => void | Promise<void>;

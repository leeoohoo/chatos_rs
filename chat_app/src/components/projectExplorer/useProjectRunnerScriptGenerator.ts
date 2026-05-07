import { useCallback } from 'react';

import type { SendMessageRuntimeOptions } from '../../lib/store/types';
import type { Project } from '../../types';
import {
  buildProjectRunnerGenerationPrompt,
  RUNNER_GENERATION_MCP_IDS,
  type ProjectRunnerMember,
} from '../../lib/domain/projectRunner';

interface ContactSessionInput {
  id: string;
  agentId: string;
  name: string;
}

interface EnsureContactSessionOptions {
  projectId: string;
  title: string;
  selectedModelId: string | null;
  projectRoot: string;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  createSessionOptions: { keepActivePanel: boolean };
}

interface UseProjectRunnerScriptGeneratorParams {
  project: Project | null;
  currentSessionId: string | null | undefined;
  selectedModelId: string | null | undefined;
  ensureContactSession: (
    contact: ContactSessionInput,
    options: EnsureContactSessionOptions,
  ) => Promise<string | null>;
  selectSession: (
    sessionId: string,
    options?: { keepActivePanel?: boolean },
  ) => Promise<void>;
  sendMessage: (
    message: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => Promise<void>;
}

export const useProjectRunnerScriptGenerator = ({
  project,
  currentSessionId,
  selectedModelId,
  ensureContactSession,
  selectSession,
  sendMessage,
}: UseProjectRunnerScriptGeneratorParams) => {
  return useCallback(async (member: ProjectRunnerMember) => {
    if (!project?.id || !project?.rootPath) {
      throw new Error('当前项目不存在或根目录为空');
    }

    const contactId = typeof member?.contactId === 'string' ? member.contactId.trim() : '';
    const contactAgentId = typeof member?.agentId === 'string' ? member.agentId.trim() : '';
    if (!contactId || !contactAgentId) {
      throw new Error('联系人信息不完整，无法生成启动脚本');
    }

    const sessionId = await ensureContactSession({
      id: contactId,
      agentId: contactAgentId,
      name: member.name,
    }, {
      projectId: project.id,
      title: member.name || '项目运行助手',
      selectedModelId: selectedModelId ?? null,
      projectRoot: project.rootPath,
      mcpEnabled: true,
      enabledMcpIds: RUNNER_GENERATION_MCP_IDS,
      createSessionOptions: { keepActivePanel: true },
    });
    if (!sessionId) {
      throw new Error('未能创建或定位联系人会话');
    }

    if (currentSessionId !== sessionId) {
      await selectSession(sessionId, { keepActivePanel: true });
    }

    await sendMessage(buildProjectRunnerGenerationPrompt(project.rootPath), [], {
      mcpEnabled: true,
      enabledMcpIds: RUNNER_GENERATION_MCP_IDS,
      contactAgentId,
      contactId,
      projectId: project.id,
      projectRoot: project.rootPath,
      workspaceRoot: null,
    });
  }, [
    currentSessionId,
    ensureContactSession,
    project?.id,
    project?.rootPath,
    selectSession,
    selectedModelId,
    sendMessage,
  ]);
};

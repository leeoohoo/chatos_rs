import type { Message, Session } from '../../types';

export interface AgentConversationGroup {
  projectId: string;
  projectName: string;
  session: Session;
}

export const normalizeProjectId = (projectId?: string | null): string => {
  const rawProjectId = typeof projectId === 'string'
    ? projectId.trim()
    : '';
  return rawProjectId || '0';
};

export const buildGroupedConversationSessions = (
  sessions: Session[],
  projectNames: Record<string, string>,
): AgentConversationGroup[] => {
  const latestByProject = new Map<string, Session>();
  for (const session of sessions) {
    const projectId = normalizeProjectId(session.project_id ?? session.projectId);
    const existing = latestByProject.get(projectId);
    if (!existing) {
      latestByProject.set(projectId, session);
      continue;
    }
    const existingTs = existing.updatedAt.getTime();
    const currentTs = session.updatedAt.getTime();
    if (Number.isNaN(existingTs) || currentTs > existingTs) {
      latestByProject.set(projectId, session);
    }
  }

  return Array.from(latestByProject.entries())
    .map(([projectId, session]) => ({
      projectId,
      projectName: projectNames[projectId]?.trim() || (projectId === '0' ? '未归属项目' : '未命名项目'),
      session,
    }))
    .sort((left, right) => right.session.updatedAt.getTime() - left.session.updatedAt.getTime());
};

export const formatMessageTime = (message: Message): string => {
  const createdAt = message.createdAt;
  if (!(createdAt instanceof Date) || Number.isNaN(createdAt.getTime())) {
    return '-';
  }
  return createdAt.toLocaleString();
};

export const getMessageRoleLabel = (role: Message['role']): string => {
  if (role === 'user') return '用户';
  if (role === 'assistant') return '助手';
  if (role === 'system') return '系统';
  if (role === 'tool') return '工具';
  return role;
};

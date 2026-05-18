import type { TranslateFn } from '../../i18n/I18nProvider';
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
  t: TranslateFn,
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
      projectName: projectNames[projectId]?.trim() || (
        projectId === '0' ? t('agentManager.session.unassignedProject') : t('agentManager.session.unnamedProject')
      ),
      session,
    }))
    .sort((left, right) => right.session.updatedAt.getTime() - left.session.updatedAt.getTime());
};

export const formatMessageTime = (message: Message, t?: TranslateFn): string => {
  const createdAt = message.createdAt;
  if (!(createdAt instanceof Date) || Number.isNaN(createdAt.getTime())) {
    return t ? t('toolSummary.na') : '-';
  }
  return createdAt.toLocaleString();
};

export const getMessageRoleLabel = (role: Message['role'], t?: TranslateFn): string => {
  if (role === 'user') return t ? t('agentManager.session.role.user') : '用户';
  if (role === 'assistant') return t ? t('agentManager.session.role.assistant') : '助手';
  if (role === 'system') return t ? t('agentManager.session.role.system') : '系统';
  if (role === 'tool') return t ? t('agentManager.session.role.tool') : '工具';
  return role;
};

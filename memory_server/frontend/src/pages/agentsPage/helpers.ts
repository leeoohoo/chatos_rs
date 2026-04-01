import type {
  MemoryAgent,
  MemorySkill,
  MemorySkillPlugin,
  Session,
} from '../../types';
import type {
  AgentConversationGroup,
  AgentPageTranslate,
} from './types';

export const normalizeProjectId = (projectId?: string | null): string => {
  const rawProjectId = typeof projectId === 'string'
    ? projectId.trim()
    : '';
  return rawProjectId || '0';
};

export const normalizeStringArray = (items: string[]): string[] => Array.from(
  new Set(items.map((item) => item.trim()).filter(Boolean)),
);

export const derivePluginSourcesForSkillIds = (
  skillIds: string[],
  skillCatalog: Record<string, MemorySkill>,
): string[] => normalizeStringArray(
  skillIds
    .map((skillId) => skillCatalog[skillId]?.plugin_source || '')
    .filter(Boolean),
);

export const mergePluginSourcesWithSkills = (
  pluginSources: string[],
  skillIds: string[],
  skillCatalog: Record<string, MemorySkill>,
): string[] => normalizeStringArray([
  ...pluginSources,
  ...derivePluginSourcesForSkillIds(skillIds, skillCatalog),
]);

export const resolvePluginDisplayName = (
  pluginSource: string,
  pluginCatalog: Record<string, MemorySkillPlugin>,
): string => {
  const normalized = pluginSource.trim();
  if (!normalized) {
    return '-';
  }
  const plugin = pluginCatalog[normalized];
  return plugin?.name?.trim() || normalized;
};

export const buildGroupedConversationSessions = (
  sessions: Session[],
  projectNames: Record<string, string>,
  t: AgentPageTranslate,
): AgentConversationGroup[] => {
  const latestByProject = new Map<string, Session>();
  for (const session of sessions) {
    const projectId = normalizeProjectId(session.project_id);
    const existing = latestByProject.get(projectId);
    if (!existing) {
      latestByProject.set(projectId, session);
      continue;
    }
    const existingTs = new Date(existing.updated_at).getTime();
    const currentTs = new Date(session.updated_at).getTime();
    if (Number.isNaN(existingTs) || currentTs > existingTs) {
      latestByProject.set(projectId, session);
    }
  }

  return Array.from(latestByProject.entries())
    .map(([projectId, session]) => ({
      projectId,
      projectName: (session.project_name || '').trim()
        || (projectNames[projectId] || '').trim()
        || (projectId === '0' ? t('memory.unassignedProject') : t('memory.unnamedProject')),
      session,
    }))
    .sort((left, right) => {
      const leftTs = new Date(left.session.updated_at || 0).getTime();
      const rightTs = new Date(right.session.updated_at || 0).getTime();
      return rightTs - leftTs;
    });
};

export const isReadonlyForScope = (
  agent: MemoryAgent,
  currentUserIdTrimmed: string,
): boolean => agent.user_id !== currentUserIdTrimmed;

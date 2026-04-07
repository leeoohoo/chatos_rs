import { useCallback, useMemo, useState } from 'react';

import { api } from '../../api/client';
import type {
  MemoryAgent,
  MemorySkill,
  MemorySkillPlugin,
  Message,
  Session,
} from '../../types';
import {
  buildGroupedConversationSessions,
  normalizeProjectId,
} from './helpers';
import type {
  AgentConversationPanelState,
  AgentPageTranslate,
  AgentPluginPreviewState,
  AgentSkillPreviewState,
} from './types';

const CONVERSATION_MESSAGES_PAGE_SIZE = 20;

interface UseAgentsPageInspectorsOptions {
  scopeUserId: string;
  t: AgentPageTranslate;
  pluginCatalog: Record<string, MemorySkillPlugin>;
  onError: (message: string) => void;
}

export interface AgentsPageInspectorsResult {
  conversationState: AgentConversationPanelState;
  openConversationDrawer: (agent: MemoryAgent) => Promise<void>;
  closeConversationDrawer: () => void;
  loadConversationMessages: (sessionId: string, page?: number) => Promise<void>;
  pluginPreviewState: AgentPluginPreviewState;
  openPluginPreview: (pluginSource: string) => Promise<void>;
  closePluginPreview: () => void;
  skillPreviewState: AgentSkillPreviewState;
  openSkillPreview: (agent: MemoryAgent, skillId: string) => Promise<void>;
  closeSkillPreview: () => void;
}

export function useAgentsPageInspectors({
  scopeUserId,
  t,
  pluginCatalog,
  onError,
}: UseAgentsPageInspectorsOptions): AgentsPageInspectorsResult {
  const [pluginPreviewOpen, setPluginPreviewOpen] = useState(false);
  const [pluginPreviewLoading, setPluginPreviewLoading] = useState(false);
  const [pluginPreviewSource, setPluginPreviewSource] = useState('');
  const [pluginPreview, setPluginPreview] = useState<MemorySkillPlugin | null>(null);
  const [pluginPreviewSkills, setPluginPreviewSkills] = useState<MemorySkill[]>([]);

  const [skillPreviewOpen, setSkillPreviewOpen] = useState(false);
  const [skillPreviewLoading, setSkillPreviewLoading] = useState(false);
  const [skillPreview, setSkillPreview] = useState<MemorySkill | null>(null);

  const [conversationOpen, setConversationOpen] = useState(false);
  const [conversationAgent, setConversationAgent] = useState<MemoryAgent | null>(null);
  const [conversationLoading, setConversationLoading] = useState(false);
  const [conversationSessions, setConversationSessions] = useState<Session[]>([]);
  const [conversationSessionId, setConversationSessionId] = useState<string | null>(null);
  const [conversationMessages, setConversationMessages] = useState<Message[]>([]);
  const [conversationMessagesLoading, setConversationMessagesLoading] = useState(false);
  const [conversationMessagesPage, setConversationMessagesPage] = useState(1);
  const [conversationMessagesHasMore, setConversationMessagesHasMore] = useState(false);
  const [conversationProjectNames, setConversationProjectNames] = useState<Record<string, string>>({});

  const groupedConversationSessions = useMemo(
    () => buildGroupedConversationSessions(conversationSessions, conversationProjectNames, t),
    [conversationProjectNames, conversationSessions, t],
  );

  const loadAllPluginSkills = useCallback(async (pluginSource: string): Promise<MemorySkill[]> => {
    const normalizedPluginSource = pluginSource.trim();
    if (!normalizedPluginSource) {
      return [];
    }

    const pageSize = 500;
    let offset = 0;
    const rows: MemorySkill[] = [];

    while (true) {
      const pageRows = await api.listSkills(scopeUserId, {
        plugin_source: normalizedPluginSource,
        limit: pageSize,
        offset,
      });
      if (pageRows.length === 0) {
        break;
      }
      rows.push(...pageRows);
      if (pageRows.length < pageSize) {
        break;
      }
      offset += pageRows.length;
    }

    return rows;
  }, [scopeUserId]);

  const openPluginPreview = async (pluginSource: string) => {
    const normalizedPluginSource = pluginSource.trim();
    if (!normalizedPluginSource) {
      return;
    }

    setPluginPreviewOpen(true);
    setPluginPreviewLoading(true);
    setPluginPreviewSource(normalizedPluginSource);
    setPluginPreview(pluginCatalog[normalizedPluginSource] ?? null);
    setPluginPreviewSkills([]);
    try {
      const [pluginDetail, skills] = await Promise.all([
        api.getSkillPlugin(normalizedPluginSource, scopeUserId).catch(() => null),
        loadAllPluginSkills(normalizedPluginSource),
      ]);
      setPluginPreview(pluginDetail || pluginCatalog[normalizedPluginSource] || null);
      setPluginPreviewSkills(skills);
    } catch (err) {
      setPluginPreviewOpen(false);
      onError((err as Error).message);
    } finally {
      setPluginPreviewLoading(false);
    }
  };

  const closePluginPreview = () => {
    setPluginPreviewOpen(false);
    setPluginPreviewSource('');
    setPluginPreview(null);
    setPluginPreviewSkills([]);
  };

  const openSkillPreview = async (agent: MemoryAgent, skillId: string) => {
    const normalizedSkillId = skillId.trim();
    if (!normalizedSkillId) {
      return;
    }

    const embedded = (agent.skills || []).find((item) => item.id === normalizedSkillId);
    setSkillPreviewOpen(true);
    setSkillPreview(null);
    setSkillPreviewLoading(true);
    try {
      try {
        const item = await api.getSkill(normalizedSkillId, scopeUserId);
        if (item) {
          setSkillPreview(item);
          return;
        }
      } catch (err) {
        if (!embedded) {
          throw err;
        }
      }

      if (embedded) {
        setSkillPreview({
          id: embedded.id,
          user_id: agent.user_id,
          plugin_source: 'inline',
          name: embedded.name || embedded.id,
          description: `Inline skill from agent ${agent.name || agent.id}`,
          content: embedded.content || '',
          source_path: '',
          version: null,
          updated_at: agent.updated_at,
        });
        return;
      }

      throw new Error(t('agents.skillNotFound'));
    } catch (err) {
      setSkillPreviewOpen(false);
      onError((err as Error).message);
    } finally {
      setSkillPreviewLoading(false);
    }
  };

  const closeSkillPreview = () => {
    setSkillPreviewOpen(false);
    setSkillPreview(null);
  };

  const loadConversationMessages = useCallback(async (sessionId: string, page = 1) => {
    const normalizedSessionId = sessionId.trim();
    if (!normalizedSessionId) {
      setConversationMessages([]);
      setConversationSessionId(null);
      setConversationMessagesPage(1);
      setConversationMessagesHasMore(false);
      return;
    }
    setConversationMessagesLoading(true);
    try {
      const limit = CONVERSATION_MESSAGES_PAGE_SIZE + 1;
      const offset = Math.max(0, (page - 1) * CONVERSATION_MESSAGES_PAGE_SIZE);
      const rows = await api.listMessages(normalizedSessionId, {
        limit,
        offset,
        order: 'asc',
      });
      const hasMore = rows.length > CONVERSATION_MESSAGES_PAGE_SIZE;
      setConversationMessages(rows.slice(0, CONVERSATION_MESSAGES_PAGE_SIZE));
      setConversationMessagesHasMore(hasMore);
      setConversationMessagesPage(page);
      setConversationSessionId(normalizedSessionId);
    } catch (err) {
      onError((err as Error).message);
      setConversationMessages([]);
      setConversationSessionId(normalizedSessionId);
      setConversationMessagesPage(page);
      setConversationMessagesHasMore(false);
    } finally {
      setConversationMessagesLoading(false);
    }
  }, [onError]);

  const openConversationDrawer = async (agent: MemoryAgent) => {
    setConversationOpen(true);
    setConversationAgent(agent);
    setConversationLoading(true);
    setConversationMessages([]);
    setConversationSessionId(null);
    setConversationMessagesPage(1);
    setConversationMessagesHasMore(false);
    setConversationProjectNames({});
    try {
      const [rows, projects] = await Promise.all([
        api.listAgentSessions(agent.id, scopeUserId, {
          limit: 120,
          offset: 0,
        }),
        api.listProjects(scopeUserId, {
          status: 'active',
          include_virtual: true,
          limit: 500,
          offset: 0,
        }),
      ]);
      const nextProjectNames: Record<string, string> = {};
      for (const project of projects) {
        const projectId = normalizeProjectId(project.project_id);
        const projectName = project.name?.trim();
        if (projectName) {
          nextProjectNames[projectId] = projectName;
        }
      }
      if (!nextProjectNames['0']) {
        nextProjectNames['0'] = t('memory.unassignedProject');
      }
      setConversationProjectNames(nextProjectNames);
      setConversationSessions(rows);
      const firstSession = rows[0];
      if (firstSession?.id) {
        await loadConversationMessages(firstSession.id, 1);
      }
    } catch (err) {
      onError((err as Error).message);
      setConversationSessions([]);
      setConversationMessages([]);
      setConversationSessionId(null);
      setConversationMessagesPage(1);
      setConversationMessagesHasMore(false);
    } finally {
      setConversationLoading(false);
    }
  };

  const closeConversationDrawer = () => {
    setConversationOpen(false);
    setConversationAgent(null);
    setConversationSessions([]);
    setConversationProjectNames({});
    setConversationSessionId(null);
    setConversationMessages([]);
    setConversationMessagesPage(1);
    setConversationMessagesHasMore(false);
  };

  const conversationState: AgentConversationPanelState = {
    open: conversationOpen,
    agent: conversationAgent,
    loading: conversationLoading,
    sessions: conversationSessions,
    sessionId: conversationSessionId,
    messages: conversationMessages,
    messagesLoading: conversationMessagesLoading,
    messagesPage: conversationMessagesPage,
    messagesPageSize: CONVERSATION_MESSAGES_PAGE_SIZE,
    messagesHasMore: conversationMessagesHasMore,
    projectNames: conversationProjectNames,
    groupedSessions: groupedConversationSessions,
  };

  const pluginPreviewState: AgentPluginPreviewState = {
    open: pluginPreviewOpen,
    loading: pluginPreviewLoading,
    source: pluginPreviewSource,
    plugin: pluginPreview,
    skills: pluginPreviewSkills,
  };

  const skillPreviewState: AgentSkillPreviewState = {
    open: skillPreviewOpen,
    loading: skillPreviewLoading,
    skill: skillPreview,
  };

  return {
    conversationState,
    openConversationDrawer,
    closeConversationDrawer,
    loadConversationMessages,
    pluginPreviewState,
    openPluginPreview,
    closePluginPreview,
    skillPreviewState,
    openSkillPreview,
    closeSkillPreview,
  };
}

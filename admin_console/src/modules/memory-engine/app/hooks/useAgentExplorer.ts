// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Form } from 'antd';
import { useEffect, useRef, useState } from 'react';

import { api } from '../../api';
import type {
  AgentDirectoryQuery,
  EngineAgentConversation,
  EngineAgentDirectoryItem,
  EngineAgentProject,
  EngineSubjectMemory,
} from '../../types';
import { textOrUndefined } from '../utils';
import { useThreadExplorer } from './useThreadExplorer';

export const DEFAULT_AGENT_FILTERS: AgentDirectoryQuery = {
  tenant_id: '',
  source_id: '',
  agent_id: '',
  limit: 100,
  offset: 0,
};

type AgentExplorerOptions = {
  onError?: (message: string) => void;
};

function agentScopeKey(agent?: EngineAgentDirectoryItem | null): string {
  if (!agent) return '';
  return `${agent.tenant_id}::${agent.source_id}::${agent.agent_subject_id}`;
}

export function buildAgentFilters(values: AgentDirectoryQuery): AgentDirectoryQuery {
  return {
    tenant_id: textOrUndefined(values.tenant_id),
    source_id: textOrUndefined(values.source_id),
    agent_id: textOrUndefined(values.agent_id),
    limit: Math.max(1, values.limit ?? 100),
    offset: 0,
  };
}

export function useAgentExplorer(options?: AgentExplorerOptions) {
  const [filterForm] = Form.useForm<AgentDirectoryQuery>();
  const [filters, setFilters] = useState<AgentDirectoryQuery>(DEFAULT_AGENT_FILTERS);
  const [agents, setAgents] = useState<EngineAgentDirectoryItem[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<EngineAgentDirectoryItem | null>(null);
  const [projects, setProjects] = useState<EngineAgentProject[]>([]);
  const [conversations, setConversations] = useState<EngineAgentConversation[]>([]);
  const [selectedConversation, setSelectedConversation] = useState<EngineAgentConversation | null>(null);
  const [selectedProject, setSelectedProject] = useState<EngineAgentProject | null>(null);
  const [agentMemories, setAgentMemories] = useState<EngineSubjectMemory[]>([]);
  const [projectMemories, setProjectMemories] = useState<EngineSubjectMemory[]>([]);
  const [agentsLoading, setAgentsLoading] = useState(false);
  const [agentDetailLoading, setAgentDetailLoading] = useState(false);
  const [projectMemoriesLoading, setProjectMemoriesLoading] = useState(false);
  const directoryRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const projectRequestRef = useRef(0);
  const conversationRequestRef = useRef(0);
  const selectedAgentKeyRef = useRef('');
  const threadExplorer = useThreadExplorer('data', {
    autoLoad: false,
    onError: options?.onError,
  });

  const reportError = (message: string) => options?.onError?.(message);

  const selectConversation = async (
    conversation: EngineAgentConversation,
    ownerAgentKey = selectedAgentKeyRef.current,
  ) => {
    const requestId = conversationRequestRef.current + 1;
    conversationRequestRef.current = requestId;
    setSelectedConversation(conversation);
    try {
      const thread = await api.getThread(conversation.thread_id, {
        tenant_id: conversation.tenant_id,
        source_id: conversation.source_id,
      });
      if (
        conversationRequestRef.current !== requestId ||
        selectedAgentKeyRef.current !== ownerAgentKey
      ) {
        return;
      }
      if (!thread) {
        reportError('该会话对应的原始线程不存在');
        return;
      }
      threadExplorer.setDetailTab('records');
      const usesConversationScope = ![
        'agent_task',
        'todo_execution',
        'agent_team_run',
      ].includes(conversation.thread_kind);
      await threadExplorer.loadThreadDetails(thread, {
        resetPage: true,
        recordScope: {
          projectId: conversation.project_subject_id ?? undefined,
          projectIdMissing: !conversation.project_subject_id,
          conversationId: usesConversationScope
            ? conversation.conversation_id ?? undefined
            : undefined,
          conversationIdMissing: usesConversationScope && !conversation.conversation_id,
        },
      });
    } catch (error) {
      if (conversationRequestRef.current === requestId) {
        reportError(`加载会话详情失败：${String(error)}`);
      }
    }
  };

  const loadProjectMemories = async (
    project: EngineAgentProject,
    ownerAgentKey = selectedAgentKeyRef.current,
  ) => {
    const requestId = projectRequestRef.current + 1;
    projectRequestRef.current = requestId;
    setSelectedProject(project);
    setProjectMemories([]);
    setProjectMemoriesLoading(true);
    try {
      const memories = await api.listSubjectMemories(project.project_subject_id, {
        tenant_id: project.tenant_id,
        source_id: project.source_id,
        memory_type: 'project_summary',
        limit: 100,
        offset: 0,
      });
      if (
        projectRequestRef.current === requestId &&
        selectedAgentKeyRef.current === ownerAgentKey
      ) {
        setProjectMemories(memories);
      }
    } catch (error) {
      if (projectRequestRef.current === requestId) {
        reportError(`加载项目总结失败：${String(error)}`);
      }
    } finally {
      if (projectRequestRef.current === requestId) {
        setProjectMemoriesLoading(false);
      }
    }
  };

  const selectAgent = async (agent: EngineAgentDirectoryItem) => {
    const requestId = detailRequestRef.current + 1;
    detailRequestRef.current = requestId;
    const key = agentScopeKey(agent);
    selectedAgentKeyRef.current = key;
    projectRequestRef.current += 1;
    conversationRequestRef.current += 1;
    threadExplorer.clearThreadDetails();
    setSelectedAgent(agent);
    setProjects([]);
    setConversations([]);
    setSelectedConversation(null);
    setSelectedProject(null);
    setAgentMemories([]);
    setProjectMemories([]);
    setAgentDetailLoading(true);
    try {
      const [nextProjects, memories, nextConversations] = await Promise.all([
        api.listAgentProjects(agent.agent_subject_id, {
          tenant_id: agent.tenant_id,
          source_id: agent.source_id,
        }),
        api.listSubjectMemories(agent.agent_subject_id, {
          tenant_id: agent.tenant_id,
          source_id: agent.source_id,
          memory_type: 'agent_memory',
          limit: 100,
          offset: 0,
        }),
        api.listAgentConversations(agent.agent_subject_id, {
          tenant_id: agent.tenant_id,
          source_id: agent.source_id,
        }),
      ]);
      if (detailRequestRef.current !== requestId || selectedAgentKeyRef.current !== key) {
        return;
      }
      setProjects(nextProjects);
      setConversations(nextConversations);
      setAgentMemories(memories);
      await Promise.all([
        nextConversations[0]
          ? selectConversation(nextConversations[0], key)
          : Promise.resolve(),
        nextProjects[0]
          ? loadProjectMemories(nextProjects[0], key)
          : Promise.resolve(),
      ]);
    } catch (error) {
      if (detailRequestRef.current === requestId) {
        reportError(`加载 Agent 记忆详情失败：${String(error)}`);
      }
    } finally {
      if (detailRequestRef.current === requestId) {
        setAgentDetailLoading(false);
      }
    }
  };

  const loadAgents = async (nextFilters = filters) => {
    const requestId = directoryRequestRef.current + 1;
    directoryRequestRef.current = requestId;
    setAgentsLoading(true);
    try {
      const items = await api.listAgents(nextFilters);
      if (directoryRequestRef.current !== requestId) return;
      setAgents(items);
      const currentKey = selectedAgentKeyRef.current;
      const nextSelected = items.find((item) => agentScopeKey(item) === currentKey) ?? items[0];
      if (nextSelected) {
        await selectAgent(nextSelected);
      } else {
        detailRequestRef.current += 1;
        projectRequestRef.current += 1;
        conversationRequestRef.current += 1;
        selectedAgentKeyRef.current = '';
        threadExplorer.clearThreadDetails();
        setSelectedAgent(null);
        setProjects([]);
        setConversations([]);
        setSelectedConversation(null);
        setSelectedProject(null);
        setAgentMemories([]);
        setProjectMemories([]);
      }
    } catch (error) {
      if (directoryRequestRef.current === requestId) {
        reportError(`加载 Agent 列表失败：${String(error)}`);
      }
    } finally {
      if (directoryRequestRef.current === requestId) {
        setAgentsLoading(false);
      }
    }
  };

  useEffect(() => {
    void loadAgents(DEFAULT_AGENT_FILTERS);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const applyFilters = async () => {
    try {
      const nextFilters = buildAgentFilters(await filterForm.validateFields());
      setFilters(nextFilters);
      await loadAgents(nextFilters);
    } catch (error) {
      const text = String(error);
      if (!text.includes('validate')) reportError(`应用 Agent 筛选失败：${text}`);
    }
  };

  const resetFilters = async () => {
    filterForm.setFieldsValue(DEFAULT_AGENT_FILTERS);
    setFilters(DEFAULT_AGENT_FILTERS);
    await loadAgents(DEFAULT_AGENT_FILTERS);
  };

  return {
    filterForm,
    filters,
    agents,
    selectedAgent,
    projects,
    conversations,
    selectedConversation,
    selectedProject,
    agentMemories,
    projectMemories,
    agentsLoading,
    agentDetailLoading,
    projectMemoriesLoading,
    loadAgents,
    selectAgent,
    selectProject: loadProjectMemories,
    selectConversation,
    applyFilters,
    resetFilters,
    threadExplorer,
  };
}

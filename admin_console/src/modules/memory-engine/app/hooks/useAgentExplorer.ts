// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Form } from 'antd';
import { useEffect, useRef, useState } from 'react';

import { api } from '../../api';
import type {
  AgentDirectoryQuery,
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
  const [selectedProject, setSelectedProject] = useState<EngineAgentProject | null>(null);
  const [agentMemories, setAgentMemories] = useState<EngineSubjectMemory[]>([]);
  const [projectMemories, setProjectMemories] = useState<EngineSubjectMemory[]>([]);
  const [agentsLoading, setAgentsLoading] = useState(false);
  const [agentDetailLoading, setAgentDetailLoading] = useState(false);
  const [projectMemoriesLoading, setProjectMemoriesLoading] = useState(false);
  const directoryRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const projectRequestRef = useRef(0);
  const selectedAgentKeyRef = useRef('');
  const threadExplorer = useThreadExplorer('data', {
    autoLoad: false,
    onError: options?.onError,
  });

  const reportError = (message: string) => options?.onError?.(message);

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
    setSelectedAgent(agent);
    setProjects([]);
    setSelectedProject(null);
    setAgentMemories([]);
    setProjectMemories([]);
    setAgentDetailLoading(true);
    try {
      const [nextProjects, memories] = await Promise.all([
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
        threadExplorer.applyThreadFilters({
          tenant_id: agent.tenant_id,
          source_id: agent.source_id,
          agent_id: agent.agent_id,
          status: '',
          limit: 50,
          offset: 0,
        }),
      ]);
      if (detailRequestRef.current !== requestId || selectedAgentKeyRef.current !== key) {
        return;
      }
      setProjects(nextProjects);
      setAgentMemories(memories);
      if (nextProjects[0]) {
        await loadProjectMemories(nextProjects[0], key);
      }
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
        selectedAgentKeyRef.current = '';
        setSelectedAgent(null);
        setProjects([]);
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
    selectedProject,
    agentMemories,
    projectMemories,
    agentsLoading,
    agentDetailLoading,
    projectMemoriesLoading,
    loadAgents,
    selectAgent,
    selectProject: loadProjectMemories,
    applyFilters,
    resetFilters,
    threadExplorer,
  };
}

import React, { useEffect, useMemo, useState } from 'react';

import { apiClient } from '../lib/api/client';
import { normalizeRawMessages } from '../lib/domain/messages';
import { normalizeProject } from '../lib/domain/projects';
import { normalizeSession } from '../lib/domain/sessions';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { AgentConfig } from '../types';
import AgentConversationPanel from './agentManager/AgentConversationPanel';
import { useDialogService } from './ui/DialogProvider';
import AgentAiCreateDialog from './agentManager/AgentAiCreateDialog';
import AgentList from './agentManager/AgentList';
import AgentManagerForm from './agentManager/AgentManagerForm';
import {
  buildGroupedConversationSessions,
  canSubmitAgentForm,
  canSubmitAiCreateAgentForm,
  getDefaultAgentAiCreateFormData,
  getDefaultAgentFormData,
  normalizeProjectId,
  toAgentFormData,
} from './agentManager/helpers';
import type {
  AgentAiCreateFormData,
  AgentConversationState,
  AgentFormData,
  AgentManagerProps,
} from './agentManager/types';

type AgentManagerWindow = Window & {
  __agentManagerInitAt__?: number;
};

const AgentManager: React.FC<AgentManagerProps> = ({ onClose, store: externalStore }) => {
  const internalStoreData = useChatStoreResolved();
  const storeData = externalStore ? externalStore() : internalStoreData;
  const {
    agents,
    aiModelConfigs,
    loadAgents,
    loadAiModelConfigs,
    createAgent,
    updateAgent,
    deleteAgent,
    aiCreateAgent,
  } = storeData;
  const { confirm, alert } = useDialogService();

  const [showForm, setShowForm] = useState(false);
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);
  const [formData, setFormData] = useState<AgentFormData>(getDefaultAgentFormData());
  const [showAiCreate, setShowAiCreate] = useState(false);
  const [aiCreateForm, setAiCreateForm] = useState<AgentAiCreateFormData>(getDefaultAgentAiCreateFormData());
  const [skillPlugins, setSkillPlugins] = useState<Awaited<ReturnType<typeof apiClient.listSkillPlugins>>>([]);
  const [skills, setSkills] = useState<Awaited<ReturnType<typeof apiClient.listSkills>>>([]);
  const [conversationState, setConversationState] = useState<AgentConversationState>({
    open: false,
    loading: false,
    agent: null,
    sessions: [],
    groupedSessions: [],
    selectedSessionId: null,
    messages: [],
    messagesLoading: false,
    projectNames: {},
  });

  useEffect(() => {
    const currentWindow = window as AgentManagerWindow;
    const last = currentWindow.__agentManagerInitAt__ || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    currentWindow.__agentManagerInitAt__ = now;
    void loadAgents({ force: true });
    void loadAiModelConfigs({ force: true });
    void Promise.all([
      apiClient.listSkillPlugins(undefined, { limit: 1000, offset: 0 }).then(setSkillPlugins),
      apiClient.listSkills(undefined, { limit: 1000, offset: 0 }).then(setSkills),
    ]).catch((error) => {
      console.error('Failed to load agent manager resources:', error);
    });
  }, [loadAgents, loadAiModelConfigs]);

  const pluginOptions = useMemo(
    () => skillPlugins.map((plugin) => ({
      value: plugin.source,
      label: [plugin.name, plugin.source, plugin.category].filter(Boolean).join(' | '),
    })),
    [skillPlugins],
  );

  const skillOptions = useMemo(() => {
    const selectedPluginSources = new Set(formData.pluginSources);
    return skills
      .filter((skill) => selectedPluginSources.size === 0 || selectedPluginSources.has(skill.plugin_source))
      .map((skill) => ({
        value: skill.id,
        label: [skill.name, skill.plugin_source].filter(Boolean).join(' | '),
      }));
  }, [formData.pluginSources, skills]);

  const resetForm = () => {
    setShowForm(false);
    setEditingAgentId(null);
    setFormData(getDefaultAgentFormData());
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmitAgentForm(formData)) {
      await alert({
        title: '信息不完整',
        message: '名称和角色定义是必填项。',
        type: 'warning',
      });
      return;
    }

    const agent: AgentConfig = {
      id: editingAgentId || '',
      name: formData.name.trim(),
      description: formData.description.trim(),
      category: formData.category.trim(),
      ai_model_config_id: '',
      enabled: formData.enabled,
      role_definition: formData.roleDefinition.trim(),
      plugin_sources: formData.pluginSources,
      skill_ids: formData.skillIds,
      default_skill_ids: formData.skillIds,
      createdAt: new Date(),
      updatedAt: new Date(),
      skills: [],
      mcp_policy: null,
      project_policy: null,
      app_ids: [],
    };

    if (editingAgentId) {
      await updateAgent(agent);
    } else {
      await createAgent(agent);
    }
    resetForm();
  };

  const handleDelete = async (agentId: string) => {
    const confirmed = await confirm({
      title: '删除智能体',
      message: '确定要删除这个智能体吗？此操作无法撤销。',
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    await deleteAgent(agentId);
  };

  const handleAiCreate = async () => {
    if (!canSubmitAiCreateAgentForm(aiCreateForm)) {
      await alert({
        title: '信息不完整',
        message: '请先填写智能体需求描述。',
        type: 'warning',
      });
      return;
    }
    await aiCreateAgent({
      model_config_id: aiCreateForm.modelConfigId || undefined,
      requirement: aiCreateForm.requirement.trim(),
      name: aiCreateForm.name.trim() || undefined,
      category: aiCreateForm.category.trim() || undefined,
      enabled: aiCreateForm.enabled,
    });
    setShowAiCreate(false);
    setAiCreateForm(getDefaultAgentAiCreateFormData());
  };

  const loadConversationMessages = async (sessionId: string) => {
    const normalizedSessionId = sessionId.trim();
    if (!normalizedSessionId) {
      setConversationState((current) => ({
        ...current,
        selectedSessionId: null,
        messages: [],
        messagesLoading: false,
      }));
      return;
    }

    setConversationState((current) => ({
      ...current,
      selectedSessionId: normalizedSessionId,
      messagesLoading: true,
    }));

    try {
      const rawMessages = await apiClient.getConversationMessages(normalizedSessionId, {
        limit: 200,
        offset: 0,
      });
      const messages = normalizeRawMessages(rawMessages, normalizedSessionId);
      setConversationState((current) => ({
        ...current,
        selectedSessionId: normalizedSessionId,
        messages,
        messagesLoading: false,
      }));
    } catch (error) {
      console.error('Failed to load agent conversation messages:', error);
      setConversationState((current) => ({
        ...current,
        selectedSessionId: normalizedSessionId,
        messages: [],
        messagesLoading: false,
      }));
    }
  };

  const openConversationPanel = async (agent: AgentConfig) => {
    setConversationState({
      open: true,
      loading: true,
      agent,
      sessions: [],
      groupedSessions: [],
      selectedSessionId: null,
      messages: [],
      messagesLoading: false,
      projectNames: {},
    });

    try {
      const [rawSessions, rawProjects] = await Promise.all([
        apiClient.getAgentSessions(agent.id, undefined, { limit: 120, offset: 0 }),
        apiClient.listProjects(),
      ]);

      const sessions = rawSessions.map((item) => normalizeSession(item));
      const projectNames = rawProjects
        .map((project) => normalizeProject(project))
        .reduce<Record<string, string>>((acc, project) => {
          const projectId = normalizeProjectId(project.id);
          const projectName = project.name?.trim();
          if (projectName) {
            acc[projectId] = projectName;
          }
          return acc;
        }, {});
      projectNames['0'] = projectNames['0'] || '未归属项目';

      const groupedSessions = buildGroupedConversationSessions(sessions, projectNames);
      setConversationState({
        open: true,
        loading: false,
        agent,
        sessions,
        groupedSessions,
        selectedSessionId: null,
        messages: [],
        messagesLoading: false,
        projectNames,
      });

      if (groupedSessions[0]?.session?.id) {
        await loadConversationMessages(groupedSessions[0].session.id);
      }
    } catch (error) {
      console.error('Failed to load agent sessions:', error);
      await alert({
        title: '加载失败',
        message: error instanceof Error ? error.message : '加载智能体会话失败',
        type: 'danger',
      });
      setConversationState({
        open: false,
        loading: false,
        agent: null,
        sessions: [],
        groupedSessions: [],
        selectedSessionId: null,
        messages: [],
        messagesLoading: false,
        projectNames: {},
      });
    }
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40" onClick={onClose} />
      <div className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[92vw] max-w-5xl h-[85vh] bg-card z-50 shadow-xl breathing-border flex flex-col rounded-lg">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <div className="flex items-center space-x-3">
            <span className="inline-block w-2.5 h-2.5 rounded-full bg-emerald-500" />
            <h2 className="text-lg font-semibold text-foreground">智能体管理</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="关闭"
          >
            关闭
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          <AgentManagerForm
            showForm={showForm}
            editingAgentId={editingAgentId}
            formData={formData}
            pluginOptions={pluginOptions}
            skillOptions={skillOptions}
            onToggleForm={() => {
              setShowForm((current) => !current);
              if (showForm) {
                resetForm();
              }
            }}
            onSubmit={handleSubmit}
            onCancel={resetForm}
            onFormDataChange={(patch) => {
              setFormData((current) => ({ ...current, ...patch }));
            }}
            onOpenAiCreate={() => setShowAiCreate(true)}
          />

          <AgentList
            agents={agents || []}
            onEdit={(agent) => {
              setEditingAgentId(agent.id);
              setFormData(toAgentFormData(agent));
              setShowForm(true);
            }}
            onDelete={handleDelete}
            onInspectSessions={openConversationPanel}
          />
        </div>
      </div>

      <AgentAiCreateDialog
        open={showAiCreate}
        formData={aiCreateForm}
        modelOptions={aiModelConfigs || []}
        onChange={(patch) => {
          setAiCreateForm((current) => ({ ...current, ...patch }));
        }}
        onCancel={() => {
          setShowAiCreate(false);
        }}
        onSubmit={handleAiCreate}
      />

      <AgentConversationPanel
        state={conversationState}
        onClose={() => {
          setConversationState({
            open: false,
            loading: false,
            agent: null,
            sessions: [],
            groupedSessions: [],
            selectedSessionId: null,
            messages: [],
            messagesLoading: false,
            projectNames: {},
          });
        }}
        onSelectSession={loadConversationMessages}
      />
    </>
  );
};

export default AgentManager;

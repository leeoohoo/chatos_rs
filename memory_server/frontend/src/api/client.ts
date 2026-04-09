import axios from 'axios';

import { buildJobConfigsApi } from './jobConfigs';
import type {
  AgentRecall,
  AiModelConfig,
  MemoryAgent,
  MemoryContact,
  ContactProject,
  MemoryProject,
  MemorySkill,
  MemorySkillPlugin,
  Message,
  ProjectMemory,
  Session,
  SessionSummary,
  TaskExecutionSummary,
  UserItem,
} from '../types';

const baseURL =
  import.meta.env.VITE_MEMORY_API_BASE ?? 'http://localhost:7080/api/memory/v1';

const client = axios.create({
  baseURL,
  timeout: 30000,
});

// AI 创建智能体可能涉及工具调用、技能筛选与多轮推理，耗时可能超过 3 分钟。
// 这里禁用 axios 超时，避免前端先断开导致网关出现 Broken pipe 噪声日志。
const AI_CREATE_AGENT_TIMEOUT_MS = 0;

type RawUserItem = {
  username: string;
  role: string;
  created_at: string;
  updated_at: string;
};

function normalizeUserItem(raw: RawUserItem): UserItem {
  return {
    username: raw.username,
    role: raw.role,
    created_at: raw.created_at,
    updated_at: raw.updated_at,
  };
}

client.interceptors.request.use((config) => {
  const authToken = localStorage.getItem('memory_auth_token');
  if (authToken) {
    config.headers.Authorization = `Bearer ${authToken}`;
  }
  return config;
});

client.interceptors.response.use(
  (response) => response,
  (error) => {
    const payload = error?.response?.data;
    const detail =
      typeof payload?.detail === 'string'
        ? payload.detail
        : typeof payload?.error === 'string'
          ? payload.error
          : typeof payload?.message === 'string'
            ? payload.message
            : null;

    if (detail) {
      return Promise.reject(new Error(detail));
    }
    if (error instanceof Error) {
      return Promise.reject(error);
    }
    return Promise.reject(new Error('request failed'));
  },
);

export const api = {
  async login(
    username: string,
    password: string,
  ): Promise<{ token: string; username: string; role: string }> {
    const { data } = await client.post('/auth/login', { username, password });
    return {
      token: data.token,
      username: data.username,
      role: data.role,
    };
  },

  async me(): Promise<{ username: string; role: string }> {
    const { data } = await client.get('/auth/me');
    return {
      username: data.username,
      role: data.role,
    };
  },

  async listUsers(limit = 500): Promise<UserItem[]> {
    const { data } = await client.get('/auth/users', { params: { limit } });
    return (data.items ?? []).map((item: RawUserItem) => normalizeUserItem(item));
  },

  async createUser(payload: { username: string; password: string; role?: string }): Promise<UserItem> {
    const { data } = await client.post('/auth/users', payload);
    return normalizeUserItem(data);
  },

  async updateUser(
    username: string,
    payload: { password?: string; role?: string },
  ): Promise<UserItem> {
    const { data } = await client.patch(`/auth/users/${username}`, payload);
    return normalizeUserItem(data);
  },

  async deleteUser(username: string): Promise<boolean> {
    const { data } = await client.delete(`/auth/users/${username}`);
    return Boolean(data?.success);
  },

  async listSessions(userId?: string): Promise<Session[]> {
    const { data } = await client.get('/sessions', {
      params: { user_id: userId, limit: 100, offset: 0 },
    });
    return data.items ?? [];
  },

  async createSession(userId: string, title?: string): Promise<Session> {
    const { data } = await client.post('/sessions', {
      user_id: userId,
      title,
    });
    return data;
  },

  async listMessages(
    sessionId: string,
    params?: { limit?: number; offset?: number; order?: 'asc' | 'desc' },
  ): Promise<Message[]> {
    const { data } = await client.get(`/sessions/${sessionId}/messages`, {
      params: {
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
        order: params?.order ?? 'asc',
      },
    });
    return data.items ?? [];
  },

  async clearSessionMessages(sessionId: string): Promise<{ deleted: number; success: boolean }> {
    const { data } = await client.delete(`/sessions/${sessionId}/messages`);
    return {
      deleted: Number(data?.deleted ?? 0),
      success: Boolean(data?.success),
    };
  },

  async createMessage(
    sessionId: string,
    payload: { role: string; content: string; message_source?: string },
  ): Promise<Message> {
    const { data } = await client.post(`/sessions/${sessionId}/messages`, payload);
    return data;
  },

  async listModelConfigs(userId: string): Promise<AiModelConfig[]> {
    const { data } = await client.get('/configs/models', { params: { user_id: userId } });
    return data.items ?? [];
  },

  async createModelConfig(payload: {
    user_id: string;
    name: string;
    provider: string;
    model: string;
    base_url?: string;
    api_key?: string;
    supports_images?: boolean;
    supports_reasoning?: boolean;
    supports_responses?: boolean;
    temperature?: number;
    thinking_level?: string;
    enabled?: boolean;
  }): Promise<AiModelConfig> {
    const { data } = await client.post('/configs/models', payload);
    return data;
  },

  async updateModelConfig(
    modelId: string,
    payload: {
      user_id: string;
      name: string;
      provider: string;
      model: string;
      base_url?: string;
      api_key?: string;
      supports_images?: boolean;
      supports_reasoning?: boolean;
      supports_responses?: boolean;
      temperature?: number;
      thinking_level?: string;
      enabled?: boolean;
    },
  ): Promise<AiModelConfig> {
    const { data } = await client.patch(`/configs/models/${modelId}`, payload);
    return data;
  },

  async deleteModelConfig(modelId: string): Promise<boolean> {
    const { data } = await client.delete(`/configs/models/${modelId}`);
    return Boolean(data?.success);
  },

  async testModelConfig(modelId: string): Promise<{ ok: boolean; output?: string; error?: string }> {
    const { data } = await client.post(`/configs/models/${modelId}/test`);
    return data;
  },

  async listAgents(
    userId?: string,
    params?: { include_shared?: boolean; enabled?: boolean; limit?: number; offset?: number },
  ): Promise<MemoryAgent[]> {
    const { data } = await client.get('/agents', {
      params: {
        user_id: userId,
        include_shared: params?.include_shared,
        enabled: params?.enabled,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async listAgentSessions(
    agentId: string,
    userId?: string,
    params?: { project_id?: string; status?: string; limit?: number; offset?: number },
  ): Promise<Session[]> {
    const normalizedStatus = params?.status?.trim();
    const { data } = await client.get(`/agents/${encodeURIComponent(agentId)}/sessions`, {
      params: {
        user_id: userId,
        project_id: params?.project_id,
        status: normalizedStatus && normalizedStatus.length > 0 ? normalizedStatus : undefined,
        limit: params?.limit ?? 100,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async createAgent(payload: {
    user_id?: string;
    name: string;
    description?: string;
    category?: string;
    model_config_id?: string;
    role_definition: string;
    plugin_sources?: string[];
    skill_ids?: string[];
    default_skill_ids?: string[];
    enabled?: boolean;
  }): Promise<MemoryAgent> {
    const { data } = await client.post('/agents', payload);
    return data;
  },

  async updateAgent(
    agentId: string,
    payload: {
      name?: string;
      description?: string;
      category?: string;
      model_config_id?: string;
      role_definition?: string;
      plugin_sources?: string[];
      skill_ids?: string[];
      default_skill_ids?: string[];
      enabled?: boolean;
    },
  ): Promise<MemoryAgent> {
    const { data } = await client.patch(`/agents/${agentId}`, payload);
    return data;
  },

  async deleteAgent(agentId: string): Promise<boolean> {
    const { data } = await client.delete(`/agents/${agentId}`);
    return Boolean(data?.success);
  },

  async aiCreateAgent(payload: {
    user_id?: string;
    model_config_id?: string;
    requirement: string;
    name?: string;
    category?: string;
    description?: string;
    role_definition?: string;
    skill_ids?: string[];
    default_skill_ids?: string[];
    enabled?: boolean;
  }): Promise<{ created: boolean; agent: MemoryAgent; source?: string }> {
    const { data } = await client.post('/agents/ai-create', payload, {
      timeout: AI_CREATE_AGENT_TIMEOUT_MS,
    });
    return data;
  },

  async listSkillPlugins(
    userId?: string,
    params?: { limit?: number; offset?: number },
  ): Promise<MemorySkillPlugin[]> {
    const { data } = await client.get('/skills/plugins', {
      params: {
        user_id: userId,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async getSkillPlugin(source: string, userId?: string): Promise<MemorySkillPlugin | null> {
    const { data } = await client.get('/skills/plugins/detail', {
      params: {
        user_id: userId,
        source,
      },
    });
    return data ?? null;
  },

  async listSkills(
    userId?: string,
    params?: { plugin_source?: string; query?: string; limit?: number; offset?: number },
  ): Promise<MemorySkill[]> {
    const { data } = await client.get('/skills', {
      params: {
        user_id: userId,
        plugin_source: params?.plugin_source,
        query: params?.query,
        limit: params?.limit ?? 300,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async getSkill(skillId: string, userId?: string): Promise<MemorySkill | null> {
    const { data } = await client.get(`/skills/${encodeURIComponent(skillId)}`, {
      params: { user_id: userId },
    });
    return data ?? null;
  },

  async importSkillsFromGit(payload: {
    user_id?: string;
    repository: string;
    branch?: string;
    marketplace_path?: string;
    plugins_path?: string;
    auto_install?: boolean;
  }): Promise<any> {
    const { data } = await client.post('/skills/import-git', payload);
    return data;
  },

  async installSkillPlugins(payload: {
    user_id?: string;
    source?: string;
    install_all?: boolean;
  }): Promise<any> {
    const { data } = await client.post('/skills/plugins/install', payload);
    return data;
  },

  async listContacts(
    userId?: string,
    params?: { limit?: number; offset?: number; status?: string },
  ): Promise<MemoryContact[]> {
    const { data } = await client.get('/contacts', {
      params: {
        user_id: userId,
        status: params?.status,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async listProjects(
    userId?: string,
    params?: { status?: string; include_virtual?: boolean; limit?: number; offset?: number },
  ): Promise<MemoryProject[]> {
    const { data } = await client.get('/projects', {
      params: {
        user_id: userId,
        status: params?.status,
        include_virtual: params?.include_virtual ?? true,
        limit: params?.limit ?? 500,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async listContactProjects(
    contactId: string,
    params?: { limit?: number; offset?: number },
  ): Promise<ContactProject[]> {
    const { data } = await client.get(
      `/contacts/${encodeURIComponent(contactId)}/projects`,
      {
        params: {
          limit: params?.limit ?? 200,
          offset: params?.offset ?? 0,
        },
      },
    );
    return data.items ?? [];
  },

  async listContactProjectMemories(
    contactId: string,
    params?: { project_id?: string; limit?: number; offset?: number },
  ): Promise<ProjectMemory[]> {
    const { data } = await client.get(
      `/contacts/${encodeURIComponent(contactId)}/project-memories`,
      {
      params: {
        project_id: params?.project_id,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
      },
      },
    );
    return data.items ?? [];
  },

  async listContactProjectMemoriesByProject(
    contactId: string,
    projectId: string,
    params?: { limit?: number; offset?: number },
  ): Promise<ProjectMemory[]> {
    const { data } = await client.get(
      `/contacts/${encodeURIComponent(contactId)}/project-memories/${encodeURIComponent(projectId)}`,
      {
        params: {
          limit: params?.limit ?? 200,
          offset: params?.offset ?? 0,
        },
      },
    );
    return data.items ?? [];
  },
  ...buildJobConfigsApi(client),

  async listContactProjectSummaries(
    contactId: string,
    projectId: string,
  ): Promise<{ session_id?: string | null; items: SessionSummary[] }> {
    const { data } = await client.get(
      `/contacts/${encodeURIComponent(contactId)}/projects/${encodeURIComponent(projectId)}/summaries`,
    );
    return {
      session_id: data.session_id ?? null,
      items: data.items ?? [],
    };
  },

  async listTaskExecutionSummaries(
    userId: string,
    contactAgentId: string,
    projectId: string,
    params?: { limit?: number; offset?: number },
  ): Promise<TaskExecutionSummary[]> {
    const { data } = await client.get('/task-executions/summaries', {
      params: {
        user_id: userId,
        contact_agent_id: contactAgentId,
        project_id: projectId,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async listContactAgentRecalls(
    contactId: string,
    params?: { limit?: number; offset?: number },
  ): Promise<AgentRecall[]> {
    const { data } = await client.get(
      `/contacts/${encodeURIComponent(contactId)}/agent-recalls`,
      {
        params: {
          limit: params?.limit ?? 200,
          offset: params?.offset ?? 0,
        },
      },
    );
    return data.items ?? [];
  },
};

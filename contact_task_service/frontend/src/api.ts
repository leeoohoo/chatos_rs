import axios from 'axios';

import type {
  AuthUser,
  ContactTask,
  MemoryContactSummary,
  MemoryProjectSummary,
  TaskExecutionMessage,
  TaskResultBrief,
} from './types';

const tokenKey = 'contact_task_service_auth_token';

const client = axios.create({
  baseURL: '/api/task-service/v1',
  timeout: 15000,
});

const memoryClient = axios.create({
  baseURL: '/api/memory/v1',
  timeout: 15000,
});

const applyAuthHeader = (config: Parameters<typeof client.interceptors.request.use>[0]) => {
  const token = localStorage.getItem(tokenKey);
  if (token) {
    config.headers = config.headers || {};
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
};

client.interceptors.request.use(applyAuthHeader);
memoryClient.interceptors.request.use(applyAuthHeader);

export const authStore = {
  getToken: () => localStorage.getItem(tokenKey),
  setToken: (token: string) => localStorage.setItem(tokenKey, token),
  clear: () => localStorage.removeItem(tokenKey),
};

export const api = {
  async login(username: string, password: string): Promise<{ token: string; username: string; role: string }> {
    const { data } = await client.post('/auth/login', { username, password });
    return data;
  },

  async me(): Promise<AuthUser> {
    const { data } = await client.get('/auth/me');
    return data;
  },

  async listTasks(params?: {
    user_id?: string;
    contact_agent_id?: string;
    project_id?: string;
    status?: string;
  }): Promise<ContactTask[]> {
    const { data } = await client.get('/tasks', { params });
    return data.items ?? [];
  },

  async listTaskExecutionMessages(taskId: string): Promise<TaskExecutionMessage[]> {
    const { data } = await client.get(`/tasks/${encodeURIComponent(taskId)}/execution-messages`);
    return data.items ?? [];
  },

  async listMemoryContacts(userId?: string): Promise<MemoryContactSummary[]> {
    const { data } = await memoryClient.get('/contacts', {
      params: {
        user_id: userId || undefined,
        limit: 2000,
        offset: 0,
      },
    });
    return data.items ?? [];
  },

  async listMemoryProjects(userId?: string): Promise<MemoryProjectSummary[]> {
    const { data } = await memoryClient.get('/projects', {
      params: {
        user_id: userId || undefined,
        include_virtual: true,
        limit: 2000,
        offset: 0,
      },
    });
    return data.items ?? [];
  },

  async getTaskResultBrief(taskId: string): Promise<TaskResultBrief | null> {
    const { data } = await client.get(`/tasks/${encodeURIComponent(taskId)}/result-brief`);
    return data.item ?? null;
  },
};

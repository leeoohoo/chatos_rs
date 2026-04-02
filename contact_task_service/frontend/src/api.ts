import axios from 'axios';

import type { AuthUser, ContactTask, TaskExecutionMessage } from './types';

const tokenKey = 'contact_task_service_auth_token';

const client = axios.create({
  baseURL: '/api/task-service/v1',
  timeout: 15000,
});

client.interceptors.request.use((config) => {
  const token = localStorage.getItem(tokenKey);
  if (token) {
    config.headers = config.headers || {};
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

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
};

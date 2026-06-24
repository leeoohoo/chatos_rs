import type {
  BatchTaskDeletePayload,
  BatchTaskOperationResponse,
  BatchTaskRunPayload,
  BatchTaskStatusUpdatePayload,
  CancelAskUserPromptPayload,
  CreateExternalMcpConfigPayload,
  CreateModelConfigPayload,
  CreateTaskPayload,
  CreateUserPayload,
  CurrentUserResponse,
  HealthResponse,
  LoginPayload,
  LoginResponse,
  ExternalMcpConfigRecord,
  McpCatalogEntry,
  McpPromptPreviewPayload,
  McpPromptPreviewResponse,
  McpServerInfo,
  CreateRemoteServerPayload,
  ModelCatalogResponse,
  ModelConfigRecord,
  ModelConfigTestResponse,
  ModelConfigUsageRecord,
  PaginatedResponse,
  PreviewModelCatalogPayload,
  PromptListFilters,
  RecordTaskProcessPayload,
  RemoteServerRecord,
  RemoteServerTestResponse,
  StartTaskRunPayload,
  SubmitAskUserPromptPayload,
  RunSummaryRecord,
  RunListFilters,
  TaskStatsResponse,
  TaskIndexResponse,
  TaskRunnerInternalPromptPreviewResponse,
  TaskRunnerSkillResponse,
  TaskMemoryContextPayload,
  TaskMemoryContextResponse,
  TaskMemoryRecordsPayload,
  TaskMemoryRecordsResponse,
  TaskMemorySummaryResponse,
  TaskSummaryRecord,
  TaskListFilters,
  TaskProjectRecord,
  TaskRecord,
  TaskRunEventRecord,
  TaskRunRecord,
  TestModelConfigPayload,
  TestRemoteServerPayload,
  ToolingNotepadFoldersResponse,
  ToolingNotepadNoteResponse,
  ToolingNotepadNotesResponse,
  ToolingNotepadTagsResponse,
  ToolingTerminalKillResponse,
  ToolingTerminalProcessListResponse,
  ToolingTerminalProcessLogsResponse,
  ToolingTerminalWriteResponse,
  AskUserPromptRecord,
  AskUserPromptStatus,
  AskUserPromptTaskCountRecord,
  UpdateModelConfigPayload,
  UpdateExternalMcpConfigPayload,
  UpdateRemoteServerPayload,
  UpdateRuntimeSettingsPayload,
  UpdateTaskPayload,
  UpdateUserPayload,
  SystemConfigResponse,
  UserSummaryRecord,
} from '../types';

const RAW_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = normalizeApiBaseUrl(RAW_API_BASE_URL);
const AUTH_TOKEN_STORAGE_KEY = 'task_runner_service_auth_token';

export function getAuthToken(): string | null {
  if (typeof window === 'undefined') {
    return null;
  }
  return window.localStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}

export function setAuthToken(token: string): void {
  if (typeof window === 'undefined') {
    return;
  }
  window.localStorage.setItem(AUTH_TOKEN_STORAGE_KEY, token);
  window.dispatchEvent(new Event('task-runner-auth-changed'));
}

export function clearAuthToken(): void {
  if (typeof window === 'undefined') {
    return;
  }
  window.localStorage.removeItem(AUTH_TOKEN_STORAGE_KEY);
  window.dispatchEvent(new Event('task-runner-auth-changed'));
}

export function buildApiUrl(path: string): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return API_BASE_URL ? `${API_BASE_URL}${normalizedPath}` : normalizedPath;
}

export function buildEventSourceUrl(path: string): string {
  const token = getAuthToken();
  if (!token) {
    return buildApiUrl(path);
  }
  const url = buildApiUrl(path);
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}access_token=${encodeURIComponent(token)}`;
}

function normalizeApiBaseUrl(value: string): string {
  if (!value) {
    return '';
  }
  const trimmed = value.replace(/\/+$/, '');
  return trimmed.endsWith('/api') ? trimmed.slice(0, -4) : trimmed;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const token = getAuthToken();
  if (token && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  const response = await fetch(buildApiUrl(path), {
    ...init,
    headers,
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const data = (await response.json()) as { error?: string };
      if (data.error) {
        message = data.error;
      }
    } catch {
      // noop
    }
    if (response.status === 401) {
      clearAuthToken();
    }
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

function withQuery(path: string, params: Record<string, string | undefined>): string {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value && value.trim()) {
      search.set(key, value);
    }
  });
  const query = search.toString();
  return query ? `${path}?${query}` : path;
}

export const api = {
  health: () => request<HealthResponse>('/api/health'),
  getSystemConfig: () => request<SystemConfigResponse>('/api/system/config'),
  getTaskRunnerSkill: (lang: string) =>
    request<TaskRunnerSkillResponse>(
      withQuery('/api/skills/task-runner', {
        lang,
      }),
    ),
  getTaskRunnerInternalPrompts: (lang: string) =>
    request<TaskRunnerInternalPromptPreviewResponse>(
      withQuery('/api/system/internal-prompts', {
        lang,
      }),
    ),
  updateSystemConfig: (payload: UpdateRuntimeSettingsPayload) =>
    request<SystemConfigResponse>('/api/system/config', {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  login: (payload: LoginPayload) =>
    request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  currentUser: () => request<CurrentUserResponse>('/api/auth/me'),
  logout: () =>
    request<void>('/api/auth/logout', {
      method: 'POST',
    }),
  listUsers: () => request<UserSummaryRecord[]>('/api/users'),
  createUser: (payload: CreateUserPayload) =>
    request<UserSummaryRecord>('/api/users', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateUser: (id: string, payload: UpdateUserPayload) =>
    request<UserSummaryRecord>(`/api/users/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteUser: (id: string) =>
    request<void>(`/api/users/${id}`, {
      method: 'DELETE',
    }),
  listTasks: (filters?: TaskListFilters) =>
    request<TaskRecord[]>(
      withQuery('/api/tasks', {
        status: filters?.status,
        keyword: filters?.keyword,
        tag: filters?.tag,
        model_config_id: filters?.model_config_id,
        project_id: filters?.project_id,
        scheduled_only:
          filters?.scheduled_only === undefined ? undefined : String(filters.scheduled_only),
        parent_task_id: filters?.parent_task_id,
        include_subtasks:
          filters?.include_subtasks === undefined ? undefined : String(filters.include_subtasks),
        source_run_id: filters?.source_run_id,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  listTasksPage: (filters?: TaskListFilters) =>
    request<PaginatedResponse<TaskRecord>>(
      withQuery('/api/tasks/page', {
        status: filters?.status,
        keyword: filters?.keyword,
        tag: filters?.tag,
        model_config_id: filters?.model_config_id,
        project_id: filters?.project_id,
        scheduled_only:
          filters?.scheduled_only === undefined ? undefined : String(filters.scheduled_only),
        parent_task_id: filters?.parent_task_id,
        include_subtasks:
          filters?.include_subtasks === undefined ? undefined : String(filters.include_subtasks),
        source_run_id: filters?.source_run_id,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  getTaskStats: () => request<TaskStatsResponse>('/api/tasks/stats'),
  getTaskIndex: () => request<TaskIndexResponse>('/api/tasks/index'),
  listTaskSummaries: (filters?: {
    ids?: string[];
    keyword?: string;
    status?: TaskListFilters['status'];
    project_id?: string;
    limit?: number;
  }) =>
    request<TaskSummaryRecord[]>(
      withQuery('/api/tasks/summaries', {
        ids: filters?.ids?.length ? filters.ids.join(',') : undefined,
        keyword: filters?.keyword,
        status: filters?.status,
        project_id: filters?.project_id,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
      }),
    ),
  listProjects: (status?: TaskProjectRecord['status']) =>
    request<TaskProjectRecord[]>(
      withQuery('/api/projects', {
        status,
      }),
    ),
  getProject: (id: string) => request<TaskProjectRecord>(`/api/projects/${id}`),
  listProjectTasks: (id: string) =>
    request<TaskRecord[]>(`/api/projects/${encodeURIComponent(id)}/tasks`),
  getTask: (id: string) => request<TaskRecord>(`/api/tasks/${id}`),
  createTask: (payload: CreateTaskPayload) =>
    request<TaskRecord>('/api/tasks', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateTask: (id: string, payload: UpdateTaskPayload) =>
    request<TaskRecord>(`/api/tasks/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  recordTaskProcess: (id: string, payload: RecordTaskProcessPayload) =>
    request<TaskRecord>(`/api/tasks/${id}/process-log`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteTask: (id: string) =>
    request<void>(`/api/tasks/${id}`, {
      method: 'DELETE',
    }),
  batchUpdateTaskStatus: (payload: BatchTaskStatusUpdatePayload) =>
    request<BatchTaskOperationResponse>('/api/tasks/batch/status', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  batchDeleteTasks: (payload: BatchTaskDeletePayload) =>
    request<BatchTaskOperationResponse>('/api/tasks/batch/delete', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  batchStartTaskRuns: (payload: BatchTaskRunPayload) =>
    request<BatchTaskOperationResponse>('/api/tasks/batch/runs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateTaskMcp: (id: string, payload: TaskRecord['mcp_config']) =>
    request<TaskRecord>(`/api/tasks/${id}/mcp`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  previewTaskMcpPrompt: (taskId: string) =>
    request<McpPromptPreviewResponse>(`/api/tasks/${taskId}/mcp/prompt-preview`),
  listModelConfigs: () => request<ModelConfigRecord[]>('/api/model-configs'),
  listModelConfigUsage: () => request<ModelConfigUsageRecord[]>('/api/model-configs/usage'),
  getModelConfig: (id: string) => request<ModelConfigRecord>(`/api/model-configs/${id}`),
  createModelConfig: (payload: CreateModelConfigPayload) =>
    request<ModelConfigRecord>('/api/model-configs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  testModelConfig: (id: string, payload: TestModelConfigPayload = {}) =>
    request<ModelConfigTestResponse>(`/api/model-configs/${id}/test`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listModelCatalog: (id: string) =>
    request<ModelCatalogResponse>(`/api/model-configs/${id}/models`),
  previewModelCatalog: (payload: PreviewModelCatalogPayload) =>
    request<ModelCatalogResponse>('/api/model-configs/catalog/preview', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateModelConfig: (id: string, payload: UpdateModelConfigPayload) =>
    request<ModelConfigRecord>(`/api/model-configs/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteModelConfig: (id: string) =>
    request<void>(`/api/model-configs/${id}`, {
      method: 'DELETE',
    }),
  listRemoteServers: () => request<RemoteServerRecord[]>('/api/remote-servers'),
  getRemoteServer: (id: string) => request<RemoteServerRecord>(`/api/remote-servers/${id}`),
  createRemoteServer: (payload: CreateRemoteServerPayload) =>
    request<RemoteServerRecord>('/api/remote-servers', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateRemoteServer: (id: string, payload: UpdateRemoteServerPayload) =>
    request<RemoteServerRecord>(`/api/remote-servers/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteRemoteServer: (id: string) =>
    request<void>(`/api/remote-servers/${id}`, {
      method: 'DELETE',
    }),
  testRemoteServerDraft: (payload: TestRemoteServerPayload) =>
    request<RemoteServerTestResponse>('/api/remote-servers/test', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  testRemoteServer: (id: string) =>
    request<RemoteServerTestResponse>(`/api/remote-servers/${id}/test`, {
      method: 'POST',
    }),
  listExternalMcpConfigs: () =>
    request<ExternalMcpConfigRecord[]>('/api/external-mcp-configs'),
  getExternalMcpConfig: (id: string) =>
    request<ExternalMcpConfigRecord>(`/api/external-mcp-configs/${id}`),
  createExternalMcpConfig: (payload: CreateExternalMcpConfigPayload) =>
    request<ExternalMcpConfigRecord>('/api/external-mcp-configs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateExternalMcpConfig: (id: string, payload: UpdateExternalMcpConfigPayload) =>
    request<ExternalMcpConfigRecord>(`/api/external-mcp-configs/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteExternalMcpConfig: (id: string) =>
    request<void>(`/api/external-mcp-configs/${id}`, {
      method: 'DELETE',
    }),
  listRuns: (filters?: RunListFilters) =>
    request<TaskRunRecord[]>(
      withQuery('/api/runs', {
        task_id: filters?.task_id,
        status: filters?.status,
        model_config_id: filters?.model_config_id,
        keyword: filters?.keyword,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  listRunsPage: (filters?: RunListFilters) =>
    request<PaginatedResponse<TaskRunRecord>>(
      withQuery('/api/runs/page', {
        task_id: filters?.task_id,
        status: filters?.status,
        model_config_id: filters?.model_config_id,
        keyword: filters?.keyword,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  listRunSummaries: (filters?: {
    ids?: string[];
    task_id?: string;
    status?: RunListFilters['status'];
    model_config_id?: string;
    keyword?: string;
    limit?: number;
  }) =>
    request<RunSummaryRecord[]>(
      withQuery('/api/runs/summaries', {
        ids: filters?.ids?.length ? filters.ids.join(',') : undefined,
        task_id: filters?.task_id,
        status: filters?.status,
        model_config_id: filters?.model_config_id,
        keyword: filters?.keyword,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
      }),
    ),
  listRunIndex: (filters?: RunListFilters) =>
    request<RunSummaryRecord[]>(
      withQuery('/api/runs/index', {
        task_id: filters?.task_id,
        status: filters?.status,
        model_config_id: filters?.model_config_id,
        keyword: filters?.keyword,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  getRun: (runId: string) => request<TaskRunRecord>(`/api/runs/${runId}`),
  listTaskRuns: (taskId: string, filters?: Omit<RunListFilters, 'task_id'>) =>
    request<TaskRunRecord[]>(
      withQuery(`/api/tasks/${taskId}/runs`, {
        status: filters?.status,
        model_config_id: filters?.model_config_id,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  getRunEvents: (runId: string) => request<TaskRunEventRecord[]>(`/api/runs/${runId}/events`),
  listRunPrompts: (
    runId: string,
    filters?: Omit<PromptListFilters, 'taskId' | 'runId'>,
  ) =>
    request<AskUserPromptRecord[]>(
      withQuery(`/api/runs/${runId}/prompts`, {
        status: filters?.status,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  startTaskRun: (taskId: string, payload: StartTaskRunPayload) =>
    request<TaskRunRecord>(`/api/tasks/${taskId}/runs`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  getTaskMemoryContext: (taskId: string, params?: TaskMemoryContextPayload) =>
    request<TaskMemoryContextResponse>(
      withQuery(`/api/tasks/${taskId}/memory/context`, {
        include_recent_records:
          params?.include_recent_records === undefined
            ? undefined
            : String(params.include_recent_records),
        include_thread_summary:
          params?.include_thread_summary === undefined
            ? undefined
            : String(params.include_thread_summary),
        include_subject_memory:
          params?.include_subject_memory === undefined
            ? undefined
            : String(params.include_subject_memory),
        recent_record_limit:
          params?.recent_record_limit === undefined
            ? undefined
            : String(params.recent_record_limit),
        summary_limit:
          params?.summary_limit === undefined ? undefined : String(params.summary_limit),
      }),
    ),
  getTaskMemoryRecords: (taskId: string, params?: TaskMemoryRecordsPayload) =>
    request<TaskMemoryRecordsResponse>(
      withQuery(`/api/tasks/${taskId}/memory/records`, {
        role: params?.role,
        record_type: params?.record_type,
        summary_status: params?.summary_status,
        limit: params?.limit === undefined ? undefined : String(params.limit),
        offset: params?.offset === undefined ? undefined : String(params.offset),
        order: params?.order,
      }),
    ),
  summarizeTaskMemory: (taskId: string) =>
    request<TaskMemorySummaryResponse>(`/api/tasks/${taskId}/memory/summarize`, {
      method: 'POST',
    }),
  cancelRun: (runId: string) =>
    request<TaskRunRecord>(`/api/runs/${runId}/cancel`, {
      method: 'POST',
    }),
  retryRun: (runId: string) =>
    request<TaskRunRecord>(`/api/runs/${runId}/retry`, {
      method: 'POST',
    }),
  listPrompts: (filters?: PromptListFilters) =>
    request<AskUserPromptRecord[]>(
      withQuery('/api/prompts', {
        task_id: filters?.taskId,
        run_id: filters?.runId,
        status: filters?.status,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  listPromptsPage: (filters?: PromptListFilters) =>
    request<PaginatedResponse<AskUserPromptRecord>>(
      withQuery('/api/prompts/page', {
        task_id: filters?.taskId,
        run_id: filters?.runId,
        status: filters?.status,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
      }),
    ),
  listPromptTaskCounts: (filters?: { status?: AskUserPromptStatus }) =>
    request<AskUserPromptTaskCountRecord[]>(
      withQuery('/api/prompts/task-counts', {
        status: filters?.status,
      }),
    ),
  getPrompt: (id: string) => request<AskUserPromptRecord>(`/api/prompts/${id}`),
  submitPrompt: (id: string, payload: SubmitAskUserPromptPayload) =>
    request<AskUserPromptRecord>(`/api/prompts/${id}/submit`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  cancelPrompt: (id: string, payload: CancelAskUserPromptPayload = {}) =>
    request<AskUserPromptRecord>(`/api/prompts/${id}/cancel`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listToolingNotepadFolders: (userId?: string) =>
    request<ToolingNotepadFoldersResponse>(
      withQuery('/api/tooling/notepad/folders', {
        user_id: userId,
      }),
    ),
  listToolingNotepadNotes: (filters?: {
    userId?: string;
    folder?: string;
    tags?: string[];
    query?: string;
    limit?: number;
    matchAny?: boolean;
    recursive?: boolean;
  }) =>
    request<ToolingNotepadNotesResponse>(
      withQuery('/api/tooling/notepad/notes', {
        user_id: filters?.userId,
        folder: filters?.folder,
        tags: filters?.tags?.length ? filters.tags.join(',') : undefined,
        query: filters?.query,
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
        match_any:
          filters?.matchAny === undefined ? undefined : String(filters.matchAny),
        recursive:
          filters?.recursive === undefined ? undefined : String(filters.recursive),
      }),
    ),
  getToolingNotepadNote: (id: string, userId?: string) =>
    request<ToolingNotepadNoteResponse>(
      withQuery(`/api/tooling/notepad/notes/${id}`, {
        user_id: userId,
      }),
    ),
  listToolingNotepadTags: (userId?: string) =>
    request<ToolingNotepadTagsResponse>(
      withQuery('/api/tooling/notepad/tags', {
        user_id: userId,
      }),
    ),
  listToolingTerminalProcesses: (filters?: {
    userId?: string;
    projectId?: string;
    includeExited?: boolean;
    limit?: number;
  }) =>
    request<ToolingTerminalProcessListResponse>(
      withQuery('/api/tooling/terminal/processes', {
        user_id: filters?.userId,
        project_id: filters?.projectId,
        include_exited:
          filters?.includeExited === undefined
            ? undefined
            : String(filters.includeExited),
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
      }),
    ),
  getToolingTerminalProcessLogs: (
    id: string,
    filters?: {
      userId?: string;
      projectId?: string;
      offset?: number;
      limit?: number;
    },
  ) =>
    request<ToolingTerminalProcessLogsResponse>(
      withQuery(`/api/tooling/terminal/processes/${id}/logs`, {
        user_id: filters?.userId,
        project_id: filters?.projectId,
        offset: filters?.offset === undefined ? undefined : String(filters.offset),
        limit: filters?.limit === undefined ? undefined : String(filters.limit),
      }),
    ),
  killToolingTerminalProcess: (
    id: string,
    payload?: { userId?: string; projectId?: string },
  ) =>
    request<ToolingTerminalKillResponse>(`/api/tooling/terminal/processes/${id}/kill`, {
      method: 'POST',
      body: JSON.stringify({
        user_id: payload?.userId,
        project_id: payload?.projectId,
      }),
    }),
  writeToolingTerminalProcess: (
    id: string,
    payload: {
      userId?: string;
      projectId?: string;
      data: string;
      submit?: boolean;
    },
  ) =>
    request<ToolingTerminalWriteResponse>(`/api/tooling/terminal/processes/${id}/write`, {
      method: 'POST',
      body: JSON.stringify({
        user_id: payload.userId,
        project_id: payload.projectId,
        data: payload.data,
        submit: payload.submit,
      }),
    }),
  getMcpServerInfo: () => request<McpServerInfo>('/api/mcp/server'),
  listMcpCatalog: () => request<McpCatalogEntry[]>('/api/mcp/tools'),
  previewMcpPrompt: (payload: McpPromptPreviewPayload) =>
    request<McpPromptPreviewResponse>('/api/mcp/prompt-preview', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
};

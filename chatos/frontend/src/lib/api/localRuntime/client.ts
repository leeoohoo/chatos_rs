// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../../types';
import type {
  SessionMessageResponse,
  AgentToolsResponse,
  AskUserPromptListResponse,
  AskUserPromptMutationPayload,
  AskUserPromptMutationResponse,
  ReviewRepairResponse,
  ReviewRepairStatusResponse,
  ProjectPlanOptions,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementExecuteResponse,
  ProjectRequirementStopResponse,
  ProjectRequirementWorkItemsOptions,
  ProjectRequirementWorkItemsResponse,
  ProjectRuntimeEnvironmentProgressResponse,
  ProjectRuntimeEnvironmentResponse,
  ProjectResponse,
  DeleteSuccessResponse,
  CreateLocalConnectorDirectoryRequest,
  CreateLocalConnectorDirectoryResponse,
  CreateLocalConnectorProjectRequest,
  LocalConnectorDeviceResponse,
  LocalConnectorDirectoryListResponse,
  LocalConnectorWorkspaceResponse,
  ConversationTaskRunnerActiveMessageTasksResponse,
  RuntimeGuidanceCommandResponse,
  StopChatResponse,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  SessionResponse,
  SessionSummariesListResponse,
  SessionRuntimeSettingsPayload,
  SessionRuntimeSettingsResponse,
  SessionUpsertPayload,
  UserMessageTurnsResponse,
  UpdateProjectRuntimeEnvironmentSettingsPayload,
  StreamChatAttachmentPayload,
  StreamChatCommandResponse,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from '../client/types';
import { requestLocalRuntime } from './bridge';
import {
  createLocalRuntimeDirectory,
  listLocalRuntimeDevices,
  listLocalRuntimeDirectory,
  listLocalRuntimeWorkspaces,
} from './connectorResources';
import {
  askUserSessionId,
  cancelLocalAskUserPrompt,
  listLocalAskUserPrompts,
  submitLocalAskUserPrompt,
} from './askUserPrompts';
import { parseLocalConnectorProjectRoot } from './projectRoot';
import {
  createLocalRuntimeProject,
  deleteLocalRuntimeProject,
  getLocalRuntimeProject,
  listLocalRuntimeProjects,
  updateLocalRuntimeProject,
} from './projects';
import {
  executeLocalProjectRequirement,
  stopLocalProjectRequirement,
} from './requirementExecution';
import {
  getLocalProjectPlan,
  listLocalProjectRequirementDocuments,
  listLocalProjectRequirementWorkItems,
} from './projectManagement';
import { readLocalSessionSelection } from './sessionMetadata';
import {
  analyzeLocalProjectRuntimeEnvironment,
  getLocalProjectRuntimeEnvironment,
  getLocalProjectRuntimeEnvironmentProgress,
  updateLocalProjectRuntimeEnvironmentSettings,
} from './runtimeEnvironment';
import {
  completeLocalTaskManagerTask,
  deleteLocalTaskManagerTask,
  getLocalActiveMessageTasks,
  getLocalTaskBoardTasks,
  getLocalTaskManagerTasks,
  updateLocalTaskManagerTask,
} from './taskBoard';
import { buildLocalUserMessageTurns } from './userMessageTurns';
import type {
  LocalRuntimeEventRecord,
  LocalRuntimeProjectRecord,
  LocalRuntimeSessionRecord,
} from './types';

const toSessionResponse = (
  record: LocalRuntimeSessionRecord,
  metadata: Record<string, unknown> = {},
): SessionResponse => ({
  id: record.id,
  title: record.title,
  user_id: record.owner_user_id,
  project_id: record.project_id,
  selected_model_id: record.selected_model_id ?? null,
  selected_agent_id: record.selected_agent_id ?? null,
  status: record.status,
  message_count: record.message_count,
  created_at: record.created_at,
  updated_at: record.updated_at,
  metadata: {
    ...metadata,
    runtime_origin: 'local_device',
  },
});

export class LocalRuntimeClient {
  async listConnectorDevices(): Promise<LocalConnectorDeviceResponse[]> {
    return listLocalRuntimeDevices();
  }

  async listConnectorWorkspaces(): Promise<LocalConnectorWorkspaceResponse[]> {
    return listLocalRuntimeWorkspaces();
  }

  async listConnectorDirectory(
    workspaceId: string,
    path?: string,
  ): Promise<LocalConnectorDirectoryListResponse> {
    return listLocalRuntimeDirectory(workspaceId, path);
  }

  async createConnectorDirectory(
    data: CreateLocalConnectorDirectoryRequest,
  ): Promise<CreateLocalConnectorDirectoryResponse> {
    return createLocalRuntimeDirectory(data);
  }

  async listProjects(): Promise<ProjectResponse[]> {
    return listLocalRuntimeProjects();
  }

  async createProject(data: CreateLocalConnectorProjectRequest): Promise<ProjectResponse> {
    return createLocalRuntimeProject(data);
  }

  async getProject(projectId: string): Promise<ProjectResponse> {
    return getLocalRuntimeProject(projectId);
  }

  async updateProject(
    projectId: string,
    data: { name?: string; root_path?: string },
  ): Promise<ProjectResponse> {
    return updateLocalRuntimeProject(projectId, data);
  }

  async deleteProject(projectId: string): Promise<DeleteSuccessResponse> {
    return deleteLocalRuntimeProject(projectId);
  }

  async prepareProject(project: Project): Promise<LocalRuntimeProjectRecord> {
    const root = parseLocalConnectorProjectRoot(project.rootPath);
    if (!root) {
      throw new Error('本地项目缺少有效的 Local Connector 工作区路径');
    }
    return requestLocalRuntime<LocalRuntimeProjectRecord>(
      `/api/local/runtime/projects/${encodeURIComponent(project.id)}`,
      {
        method: 'PUT',
        body: JSON.stringify({
          project_name: project.name,
          workspace_id: root.workspaceId,
          root_relative_path: root.relativePath,
        }),
      },
    );
  }

  async getProjectPlan(
    projectId: string,
    options: ProjectPlanOptions = {},
  ): Promise<ProjectPlanResponse> {
    return getLocalProjectPlan(projectId, options);
  }

  async listProjectRequirementWorkItems(
    projectId: string,
    requirementId: string,
    options: ProjectRequirementWorkItemsOptions = {},
  ): Promise<ProjectRequirementWorkItemsResponse> {
    return listLocalProjectRequirementWorkItems(projectId, requirementId, options);
  }

  async listProjectRequirementDocuments(
    projectId: string,
    requirementId: string,
  ): Promise<ProjectRequirementDocumentResponse[]> {
    return listLocalProjectRequirementDocuments(projectId, requirementId);
  }

  async executeProjectRequirement(
    projectId: string,
    requirementId: string,
    data: {
      contact_id?: string;
      include_prerequisite_dependents?: boolean;
      includePrerequisiteDependents?: boolean;
    } = {},
  ): Promise<ProjectRequirementExecuteResponse> {
    return executeLocalProjectRequirement(projectId, requirementId, data);
  }

  async stopProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data: { contact_id?: string } = {},
  ): Promise<ProjectRequirementStopResponse> {
    return stopLocalProjectRequirement(projectId, requirementId, data);
  }

  async getProjectRuntimeEnvironment(
    projectId: string,
  ): Promise<ProjectRuntimeEnvironmentResponse> {
    return getLocalProjectRuntimeEnvironment(projectId);
  }

  async updateProjectRuntimeEnvironmentSettings(
    projectId: string,
    data: UpdateProjectRuntimeEnvironmentSettingsPayload,
  ): Promise<ProjectRuntimeEnvironmentResponse> {
    return updateLocalProjectRuntimeEnvironmentSettings(projectId, data);
  }

  async analyzeProjectRuntimeEnvironment(
    projectId: string,
  ): Promise<ProjectRuntimeEnvironmentResponse> {
    return analyzeLocalProjectRuntimeEnvironment(projectId);
  }

  async getProjectRuntimeEnvironmentProgress(
    projectId: string,
  ): Promise<ProjectRuntimeEnvironmentProgressResponse> {
    return getLocalProjectRuntimeEnvironmentProgress(projectId);
  }

  async getSessions(projectId: string): Promise<SessionResponse[]> {
    const records = await requestLocalRuntime<LocalRuntimeSessionRecord[]>(
      `/api/local/runtime/sessions?project_id=${encodeURIComponent(projectId)}`,
    );
    return records.map((record) => toSessionResponse(record));
  }

  async createSession(data: SessionUpsertPayload): Promise<SessionResponse> {
    const selection = readLocalSessionSelection(data.metadata);
    const record = await requestLocalRuntime<LocalRuntimeSessionRecord>(
      '/api/local/runtime/sessions',
      {
        method: 'POST',
        body: JSON.stringify({
          project_id: data.project_id,
          title: data.title,
          selected_model_id: selection.selectedModelId,
          selected_agent_id: selection.selectedAgentId,
        }),
      },
    );
    return toSessionResponse(record, selection.metadata);
  }

  async getSession(sessionId: string): Promise<SessionResponse> {
    const record = await requestLocalRuntime<LocalRuntimeSessionRecord>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}`,
    );
    return toSessionResponse(record);
  }

  async getMessages(sessionId: string): Promise<SessionMessageResponse[]> {
    return requestLocalRuntime<SessionMessageResponse[]>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/messages`,
    );
  }

  async getUserMessageTurns(
    sessionId: string,
    options: { limit?: number; before?: string | null } = {},
  ): Promise<UserMessageTurnsResponse> {
    const [messages, taskResponse] = await Promise.all([
      this.getMessages(sessionId),
      getLocalTaskBoardTasks(sessionId, { includeDone: true, limit: 200 }),
    ]);
    return buildLocalUserMessageTurns(messages, taskResponse.items || [], options);
  }

  async getActiveMessageTasks(
    sessionId: string,
  ): Promise<ConversationTaskRunnerActiveMessageTasksResponse> {
    return getLocalActiveMessageTasks(sessionId);
  }

  async getTaskManagerTasks(
    sessionId: string,
    options: { conversationTurnId?: string; includeDone?: boolean; limit?: number } = {},
  ): Promise<TaskManagerTaskResponse[]> {
    return getLocalTaskManagerTasks(sessionId, options);
  }

  async updateTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload: TaskManagerUpdatePayload,
  ): Promise<TaskManagerTaskResponse> {
    return updateLocalTaskManagerTask(sessionId, taskId, payload);
  }

  async completeTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload: Partial<TaskManagerUpdatePayload> = {},
  ): Promise<TaskManagerTaskResponse> {
    return completeLocalTaskManagerTask(sessionId, taskId, payload);
  }

  async deleteTaskManagerTask(
    sessionId: string,
    taskId: string,
  ): Promise<{ success?: boolean }> {
    return deleteLocalTaskManagerTask(sessionId, taskId);
  }

  async getRuntimeEvents(
    sessionId: string,
    options: { turnId?: string | null; after?: number; limit?: number } = {},
  ): Promise<LocalRuntimeEventRecord[]> {
    const query = new URLSearchParams();
    const turnId = String(options.turnId || '').trim();
    if (turnId) {
      query.set('turn_id', turnId);
    }
    if (Number.isFinite(options.after)) {
      query.set('after', String(Math.max(0, Math.trunc(options.after || 0))));
    }
    if (Number.isFinite(options.limit)) {
      query.set('limit', String(Math.max(1, Math.trunc(options.limit || 100))));
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : '';
    return requestLocalRuntime<LocalRuntimeEventRecord[]>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/events${suffix}`,
    );
  }

  async getConversationSummaries(
    sessionId: string,
    options: { limit?: number; offset?: number } = {},
  ): Promise<SessionSummariesListResponse> {
    const query = new URLSearchParams();
    if (Number.isFinite(options.limit)) {
      query.set('limit', String(Math.max(1, Math.trunc(options.limit || 100))));
    }
    if (Number.isFinite(options.offset)) {
      query.set('offset', String(Math.max(0, Math.trunc(options.offset || 0))));
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : '';
    return requestLocalRuntime<SessionSummariesListResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/summaries${suffix}`,
    );
  }

  async deleteConversationSummary(
    sessionId: string,
    summaryId: string,
  ): Promise<{ success?: boolean }> {
    return requestLocalRuntime<{ success?: boolean }>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/summaries/${encodeURIComponent(summaryId)}`,
      { method: 'DELETE' },
    );
  }

  async clearConversationSummaries(sessionId: string): Promise<{ success?: boolean }> {
    return requestLocalRuntime<{ success?: boolean }>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/summaries`,
      { method: 'DELETE' },
    );
  }

  async runConversationReviewRepair(sessionId: string): Promise<ReviewRepairResponse> {
    return requestLocalRuntime<ReviewRepairResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/review-repair`,
      { method: 'POST' },
    );
  }

  async getConversationReviewRepairStatus(
    sessionId: string,
  ): Promise<ReviewRepairStatusResponse> {
    return requestLocalRuntime<ReviewRepairStatusResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/review-repair`,
    );
  }

  async getConversationMemoryRecalls(
    sessionId: string,
    options: { limit?: number } = {},
  ): Promise<unknown[]> {
    const query = new URLSearchParams();
    if (Number.isFinite(options.limit)) {
      query.set('limit', String(Math.max(1, Math.trunc(options.limit || 20))));
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : '';
    return requestLocalRuntime<unknown[]>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/memory-recalls${suffix}`,
    );
  }

  async deleteConversationMemoryRecall(
    sessionId: string,
    recallId: string,
  ): Promise<{ success?: boolean; deleted_recalls?: number }> {
    return requestLocalRuntime<{ success?: boolean; deleted_recalls?: number }>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/memory-recalls/${encodeURIComponent(recallId)}`,
      { method: 'DELETE' },
    );
  }

  async getRuntimeSettings(sessionId: string): Promise<SessionRuntimeSettingsResponse> {
    return requestLocalRuntime<SessionRuntimeSettingsResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/runtime-settings`,
    );
  }

  async getAgentTools(sessionId: string): Promise<AgentToolsResponse> {
    return requestLocalRuntime<AgentToolsResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/tools`,
    );
  }

  async listAskUserPrompts(
    sessionId: string,
    options: { includePending?: boolean; limit?: number } = {},
  ): Promise<AskUserPromptListResponse> {
    return listLocalAskUserPrompts(sessionId, options);
  }

  async submitAskUserPrompt(
    promptId: string,
    payload: AskUserPromptMutationPayload,
  ): Promise<AskUserPromptMutationResponse> {
    return submitLocalAskUserPrompt(askUserSessionId(payload), promptId, payload);
  }

  async cancelAskUserPrompt(
    promptId: string,
    payload: Pick<AskUserPromptMutationPayload, 'conversation_id' | 'conversationId' | 'reason'>,
  ): Promise<AskUserPromptMutationResponse> {
    return cancelLocalAskUserPrompt(askUserSessionId(payload), promptId, payload);
  }

  async updateRuntimeSettings(
    sessionId: string,
    data: SessionRuntimeSettingsPayload,
  ): Promise<SessionRuntimeSettingsResponse> {
    return requestLocalRuntime<SessionRuntimeSettingsResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/runtime-settings`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      },
    );
  }

  async sendChatCommand(
    conversationId: string,
    content: string,
    modelConfig: StreamChatModelConfigPayload,
    attachments?: StreamChatAttachmentPayload[],
    reasoningEnabled?: boolean,
    options?: StreamChatOptions,
  ): Promise<StreamChatCommandResponse> {
    return requestLocalRuntime<StreamChatCommandResponse>(
      '/api/local/runtime/chat/send',
      {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          content,
          attachments: attachments || [],
          reasoning_enabled: reasoningEnabled,
          turn_id: options?.turnId,
          idempotency_key: options?.turnId,
          model_config_id: modelConfig.id,
          system_prompt: options?.systemPrompt || undefined,
          ai_model_config: {
            temperature: modelConfig.temperature,
            model_name: modelConfig.model_name,
            thinking_level: modelConfig.thinking_level || null,
          },
        }),
      },
    );
  }

  async sendRuntimeGuidance(
    conversationId: string,
    turnId: string,
    content: string,
    attachments?: StreamChatAttachmentPayload[],
  ): Promise<RuntimeGuidanceCommandResponse> {
    return requestLocalRuntime<RuntimeGuidanceCommandResponse>(
      '/api/local/runtime/chat/guidance',
      {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          turn_id: turnId,
          content,
          attachments: attachments || [],
        }),
      },
    );
  }

  async stopChat(conversationId: string, turnId?: string | null): Promise<StopChatResponse> {
    return requestLocalRuntime<StopChatResponse>(
      '/api/local/runtime/chat/stop',
      {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          turn_id: turnId || undefined,
        }),
      },
    );
  }
}

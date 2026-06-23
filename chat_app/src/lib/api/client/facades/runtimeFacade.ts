import * as accountApi from '../account';
import * as notepadApi from '../notepad';
import * as streamApi from '../stream';
import * as summaryApi from '../summary';
import * as tasksApi from '../tasks';
import { buildQuery } from '../shared';
import type {
  AuthResponse,
  MeResponse,
  NotepadCreatePayload,
  NotepadDeleteNoteResponse,
  NotepadFolderMutationResponse,
  NotepadFoldersResponse,
  NotepadInitResponse,
  NotepadListOptions,
  NotepadNoteDetailResponse,
  NotepadNotesResponse,
  NotepadSearchOptions,
  NotepadTagsResponse,
  NotepadUpdatePayload,
  RegisterPayload,
  AgentToolsResponse,
  CreateTaskRunnerExternalMcpConfigPayload,
  ReviewRepairResponse,
  ReviewRepairStatusResponse,
  SessionSummariesListResponse,
  StreamChatAttachmentPayload,
  StreamChatCommandResponse,
  StreamChatModelConfigPayload,
  StreamChatOptions,
  TaskRunnerExternalMcpConfig,
  TaskManagerTaskResponse,
  TaskRunnerAgentAccountResponse,
  TaskManagerUpdatePayload,
  UpdateTaskRunnerExternalMcpConfigPayload,
  UserSettingsResponse,
} from '../types';
import type ApiClient from '../../client';

export interface RuntimeFacade {
  sendChatCommand(
    conversationId: string,
    content: string,
    modelConfig: StreamChatModelConfigPayload,
    userId?: string,
    attachments?: StreamChatAttachmentPayload[],
    reasoningEnabled?: boolean,
    options?: StreamChatOptions,
  ): Promise<StreamChatCommandResponse>;
  getAgentTools(options?: {
    conversationId?: string | null;
    mcpEnabled?: boolean;
    enabledMcpIds?: string[];
    projectId?: string | null;
    projectRoot?: string | null;
    contactAgentId?: string | null;
    skillsEnabled?: boolean;
    selectedSkillIds?: string[];
  }): Promise<AgentToolsResponse>;
  getTaskManagerTasks(
    conversationId: string,
    options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number },
  ): Promise<TaskManagerTaskResponse[]>;
  updateTaskManagerTask(
    conversationId: string,
    taskId: string,
    payload: TaskManagerUpdatePayload,
  ): Promise<TaskManagerTaskResponse>;
  completeTaskManagerTask(
    conversationId: string,
    taskId: string,
    payload?: Partial<TaskManagerUpdatePayload>,
  ): Promise<TaskManagerTaskResponse>;
  deleteTaskManagerTask(conversationId: string, taskId: string): Promise<{ success?: boolean }>;
  notepadInit(): Promise<NotepadInitResponse>;
  listNotepadFolders(): Promise<NotepadFoldersResponse>;
  createNotepadFolder(payload: { folder: string }): Promise<NotepadFolderMutationResponse>;
  renameNotepadFolder(payload: { from: string; to: string }): Promise<NotepadFolderMutationResponse>;
  deleteNotepadFolder(options: { folder: string; recursive?: boolean }): Promise<NotepadFolderMutationResponse>;
  listNotepadNotes(options?: NotepadListOptions): Promise<NotepadNotesResponse>;
  createNotepadNote(payload: NotepadCreatePayload): Promise<NotepadNoteDetailResponse>;
  getNotepadNote(noteId: string): Promise<NotepadNoteDetailResponse>;
  updateNotepadNote(noteId: string, payload: NotepadUpdatePayload): Promise<NotepadNoteDetailResponse>;
  deleteNotepadNote(noteId: string): Promise<NotepadDeleteNoteResponse>;
  listNotepadTags(): Promise<NotepadTagsResponse>;
  searchNotepadNotes(options: NotepadSearchOptions): Promise<NotepadNotesResponse>;
  getConversationSummaries(
    conversationId: string,
    options?: { limit?: number; offset?: number },
  ): Promise<SessionSummariesListResponse>;
  deleteConversationSummary(conversationId: string, summaryId: string): Promise<{ success?: boolean }>;
  clearConversationSummaries(conversationId: string): Promise<{ success?: boolean }>;
  runConversationReviewRepair(conversationId: string): Promise<ReviewRepairResponse>;
  getConversationReviewRepairStatus(conversationId: string): Promise<ReviewRepairStatusResponse>;
  register(data: RegisterPayload): Promise<AuthResponse>;
  login(data: RegisterPayload): Promise<AuthResponse>;
  getMe(): Promise<MeResponse>;
  listTaskRunnerAgentAccounts(): Promise<TaskRunnerAgentAccountResponse[]>;
  listTaskRunnerExternalMcpConfigs(): Promise<TaskRunnerExternalMcpConfig[]>;
  createTaskRunnerExternalMcpConfig(
    payload: CreateTaskRunnerExternalMcpConfigPayload,
  ): Promise<TaskRunnerExternalMcpConfig>;
  updateTaskRunnerExternalMcpConfig(
    id: string,
    payload: UpdateTaskRunnerExternalMcpConfigPayload,
  ): Promise<TaskRunnerExternalMcpConfig>;
  deleteTaskRunnerExternalMcpConfig(id: string): Promise<Record<string, never>>;
  getUserSettings(userId?: string): Promise<UserSettingsResponse>;
  updateUserSettings(userId: string, settings: Record<string, unknown>): Promise<UserSettingsResponse>;
}

export const runtimeFacade: RuntimeFacade & ThisType<ApiClient> = {
  async sendChatCommand(conversationId, content, modelConfig, userId, attachments, reasoningEnabled, options) {
    return streamApi.sendChatCommand(
      this.getStreamApiContext(),
      conversationId,
      content,
      modelConfig,
      userId,
      attachments,
      reasoningEnabled,
      options,
    );
  },
  async getAgentTools(options) {
    const query = buildQuery({
      conversation_id: options?.conversationId,
      mcp_enabled: typeof options?.mcpEnabled === 'boolean' ? options.mcpEnabled : undefined,
      enabled_mcp_ids: Array.isArray(options?.enabledMcpIds)
        ? options.enabledMcpIds.join(',')
        : undefined,
      project_id: options?.projectId,
      project_root: options?.projectRoot,
      contact_agent_id: options?.contactAgentId,
      skills_enabled: typeof options?.skillsEnabled === 'boolean' ? options.skillsEnabled : undefined,
      selected_skill_ids: Array.isArray(options?.selectedSkillIds) && options?.selectedSkillIds.length > 0
        ? options?.selectedSkillIds.join(',')
        : undefined,
    });
    return this.getRequestFn()<AgentToolsResponse>(`/agent/tools${query}`);
  },
  async getTaskManagerTasks(conversationId, options) {
    return tasksApi.getTaskManagerTasks(this.getRequestFn(), conversationId, options);
  },
  async updateTaskManagerTask(conversationId, taskId, payload) {
    return tasksApi.updateTaskManagerTask(this.getRequestFn(), conversationId, taskId, payload);
  },
  async completeTaskManagerTask(conversationId, taskId, payload) {
    return tasksApi.completeTaskManagerTask(this.getRequestFn(), conversationId, taskId, payload);
  },
  async deleteTaskManagerTask(conversationId, taskId) {
    return tasksApi.deleteTaskManagerTask(this.getRequestFn(), conversationId, taskId);
  },
  async notepadInit() {
    return notepadApi.notepadInit(this.getRequestFn());
  },
  async listNotepadFolders() {
    return notepadApi.listNotepadFolders(this.getRequestFn());
  },
  async createNotepadFolder(payload) {
    return notepadApi.createNotepadFolder(this.getRequestFn(), payload);
  },
  async renameNotepadFolder(payload) {
    return notepadApi.renameNotepadFolder(this.getRequestFn(), payload);
  },
  async deleteNotepadFolder(options) {
    return notepadApi.deleteNotepadFolder(this.getRequestFn(), options);
  },
  async listNotepadNotes(options) {
    return notepadApi.listNotepadNotes(this.getRequestFn(), options);
  },
  async createNotepadNote(payload) {
    return notepadApi.createNotepadNote(this.getRequestFn(), payload);
  },
  async getNotepadNote(noteId) {
    return notepadApi.getNotepadNote(this.getRequestFn(), noteId);
  },
  async updateNotepadNote(noteId, payload) {
    return notepadApi.updateNotepadNote(this.getRequestFn(), noteId, payload);
  },
  async deleteNotepadNote(noteId) {
    return notepadApi.deleteNotepadNote(this.getRequestFn(), noteId);
  },
  async listNotepadTags() {
    return notepadApi.listNotepadTags(this.getRequestFn());
  },
  async searchNotepadNotes(options) {
    return notepadApi.searchNotepadNotes(this.getRequestFn(), options);
  },
  async getConversationSummaries(conversationId, options) {
    return summaryApi.getConversationSummaries(this.getRequestFn(), conversationId, options);
  },
  async deleteConversationSummary(conversationId, summaryId) {
    return summaryApi.deleteConversationSummary(this.getRequestFn(), conversationId, summaryId);
  },
  async clearConversationSummaries(conversationId) {
    return summaryApi.clearConversationSummaries(this.getRequestFn(), conversationId);
  },
  async runConversationReviewRepair(conversationId) {
    return summaryApi.runConversationReviewRepair(this.getRequestFn(), conversationId);
  },
  async getConversationReviewRepairStatus(conversationId) {
    return summaryApi.getConversationReviewRepairStatus(this.getRequestFn(), conversationId);
  },
  async register(data) {
    return accountApi.register(this.getRequestFn(), data);
  },
  async login(data) {
    return accountApi.login(this.getRequestFn(), data);
  },
  async getMe() {
    return accountApi.getMe(this.getRequestFn());
  },
  async listTaskRunnerAgentAccounts() {
    return accountApi.listTaskRunnerAgentAccounts(this.getRequestFn());
  },
  async listTaskRunnerExternalMcpConfigs() {
    return this.getRequestFn()<TaskRunnerExternalMcpConfig[]>('/task-runner/external-mcp-configs');
  },
  async createTaskRunnerExternalMcpConfig(payload) {
    return this.getRequestFn()<TaskRunnerExternalMcpConfig>('/task-runner/external-mcp-configs', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  },
  async updateTaskRunnerExternalMcpConfig(id, payload) {
    return this.getRequestFn()<TaskRunnerExternalMcpConfig>(
      `/task-runner/external-mcp-configs/${encodeURIComponent(id)}`,
      {
        method: 'PATCH',
        body: JSON.stringify(payload),
      },
    );
  },
  async deleteTaskRunnerExternalMcpConfig(id) {
    return this.getRequestFn()<Record<string, never>>(
      `/task-runner/external-mcp-configs/${encodeURIComponent(id)}`,
      {
        method: 'DELETE',
      },
    );
  },
  async getUserSettings(userId) {
    return accountApi.getUserSettings(this.getRequestFn(), userId);
  },
  async updateUserSettings(userId, settings) {
    return accountApi.updateUserSettings(this.getRequestFn(), userId, settings);
  },
};

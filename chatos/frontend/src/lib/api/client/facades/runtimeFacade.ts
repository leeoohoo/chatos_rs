// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as accountApi from '../account';
import * as notepadApi from '../notepad';
import * as streamApi from '../stream';
import * as summaryApi from '../summary';
import * as tasksApi from '../tasks';
import * as askUserPromptsApi from '../askUserPrompts';
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
  ReviewRepairResponse,
  ReviewRepairStatusResponse,
  SessionSummariesListResponse,
  RuntimeGuidanceCommandResponse,
  StreamChatAttachmentPayload,
  StreamChatCommandResponse,
  StreamChatModelConfigPayload,
  StreamChatOptions,
  TaskManagerTaskResponse,
  TaskRunnerAgentAccountResponse,
  TaskManagerUpdatePayload,
  AskUserPromptListResponse,
  AskUserPromptMutationPayload,
  AskUserPromptMutationResponse,
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
  sendRuntimeGuidance(
    conversationId: string,
    turnId: string,
    content: string,
    attachments?: StreamChatAttachmentPayload[],
  ): Promise<RuntimeGuidanceCommandResponse>;
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
  listAskUserPrompts(
    conversationId: string,
    options?: { includePending?: boolean; limit?: number },
  ): Promise<AskUserPromptListResponse>;
  submitAskUserPrompt(
    promptId: string,
    payload: AskUserPromptMutationPayload,
  ): Promise<AskUserPromptMutationResponse>;
  cancelAskUserPrompt(
    promptId: string,
    payload: Pick<AskUserPromptMutationPayload, 'conversation_id' | 'conversationId' | 'reason'>,
  ): Promise<AskUserPromptMutationResponse>;
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
  async sendRuntimeGuidance(conversationId, turnId, content, attachments) {
    return this.getRequestFn()<RuntimeGuidanceCommandResponse>('/agent/chat/guidance', {
      method: 'POST',
      body: JSON.stringify({
        conversation_id: conversationId,
        turn_id: turnId,
        content,
        attachments: attachments || [],
      }),
    });
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
  async listAskUserPrompts(conversationId, options) {
    return askUserPromptsApi.listAskUserPrompts(this.getRequestFn(), conversationId, options);
  },
  async submitAskUserPrompt(promptId, payload) {
    return askUserPromptsApi.submitAskUserPrompt(this.getRequestFn(), promptId, payload);
  },
  async cancelAskUserPrompt(promptId, payload) {
    return askUserPromptsApi.cancelAskUserPrompt(this.getRequestFn(), promptId, payload);
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
  async getUserSettings(userId) {
    return accountApi.getUserSettings(this.getRequestFn(), userId);
  },
  async updateUserSettings(userId, settings) {
    return accountApi.updateUserSettings(this.getRequestFn(), userId, settings);
  },
};

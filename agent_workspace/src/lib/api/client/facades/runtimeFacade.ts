import * as accountApi from '../account';
import * as notepadApi from '../notepad';
import * as streamApi from '../stream';
import * as summaryApi from '../summary';
import * as tasksApi from '../tasks';
import type {
  AuthResponse,
  ImActionRequestSubmitResponse,
  ImConversationActionRequestResponse,
  ImConversationCreatePayload,
  ImConversationResponse,
  ImConversationMessageCreatePayload,
  ImConversationMessageResponse,
  ImConversationRunResponse,
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
  RuntimeGuidanceSubmitPayload,
  RuntimeGuidanceSubmitResponse,
  SessionSummariesListResponse,
  SessionSummaryJobConfigPayload,
  SessionSummaryJobConfigResponse,
  StopChatResponse,
  TaskExecutionRollupJobConfigPayload,
  TaskExecutionRollupJobConfigResponse,
  TaskExecutionSummaryJobConfigPayload,
  TaskExecutionSummaryJobConfigResponse,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  TaskReviewDecisionPayload,
  UiPromptItemResponse,
  UiPromptResponsePayload,
  UserSettingsResponse,
} from '../types';
import type ApiClient from '../../client';

export interface RuntimeFacade {
  submitRuntimeGuidance(payload: RuntimeGuidanceSubmitPayload): Promise<RuntimeGuidanceSubmitResponse>;
  getTaskManagerTasks(
    sessionId: string,
    options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number },
  ): Promise<TaskManagerTaskResponse[]>;
  updateTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload: TaskManagerUpdatePayload,
  ): Promise<TaskManagerTaskResponse>;
  confirmTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload?: { note?: string },
  ): Promise<TaskManagerTaskResponse>;
  pauseTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload?: { reason?: string },
  ): Promise<TaskManagerTaskResponse>;
  resumeTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload?: { note?: string },
  ): Promise<TaskManagerTaskResponse>;
  completeTaskManagerTask(sessionId: string, taskId: string): Promise<TaskManagerTaskResponse>;
  deleteTaskManagerTask(sessionId: string, taskId: string): Promise<{ success?: boolean }>;
  submitTaskReviewDecision(
    reviewId: string,
    payload: TaskReviewDecisionPayload,
  ): Promise<{ success?: boolean; status?: string }>;
  getImConversationActionRequests(
    conversationId: string,
  ): Promise<ImConversationActionRequestResponse[]>;
  getImConversations(): Promise<ImConversationResponse[]>;
  getImWsMeta(): Promise<{ ws_url?: string | null }>;
  createImConversation(
    payload: ImConversationCreatePayload,
  ): Promise<ImConversationResponse>;
  markImConversationRead(
    conversationId: string,
  ): Promise<ImConversationResponse>;
  getImConversationMessages(
    conversationId: string,
    options?: { limit?: number; order?: 'asc' | 'desc' },
  ): Promise<ImConversationMessageResponse[]>;
  createImConversationMessage(
    conversationId: string,
    payload: ImConversationMessageCreatePayload,
  ): Promise<ImConversationMessageResponse>;
  getImConversationRuns(
    conversationId: string,
  ): Promise<ImConversationRunResponse[]>;
  submitImActionRequest(
    actionRequestId: string,
    payload: unknown,
  ): Promise<ImActionRequestSubmitResponse>;
  getPendingUiPrompts(sessionId: string, options?: { limit?: number }): Promise<UiPromptItemResponse[]>;
  getUiPromptHistory(
    sessionId: string,
    options?: { limit?: number; includePending?: boolean },
  ): Promise<UiPromptItemResponse[]>;
  submitUiPromptResponse(
    promptId: string,
    payload: UiPromptResponsePayload,
  ): Promise<{ success?: boolean; status?: string }>;
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
  getSessionSummaryJobConfig(userId?: string): Promise<SessionSummaryJobConfigResponse>;
  updateSessionSummaryJobConfig(payload: SessionSummaryJobConfigPayload): Promise<SessionSummaryJobConfigResponse>;
  patchSessionSummaryJobConfig(payload: SessionSummaryJobConfigPayload): Promise<SessionSummaryJobConfigResponse>;
  getTaskExecutionSummaryJobConfig(userId?: string): Promise<TaskExecutionSummaryJobConfigResponse>;
  updateTaskExecutionSummaryJobConfig(payload: TaskExecutionSummaryJobConfigPayload): Promise<TaskExecutionSummaryJobConfigResponse>;
  patchTaskExecutionSummaryJobConfig(payload: TaskExecutionSummaryJobConfigPayload): Promise<TaskExecutionSummaryJobConfigResponse>;
  getTaskExecutionRollupJobConfig(userId?: string): Promise<TaskExecutionRollupJobConfigResponse>;
  updateTaskExecutionRollupJobConfig(payload: TaskExecutionRollupJobConfigPayload): Promise<TaskExecutionRollupJobConfigResponse>;
  patchTaskExecutionRollupJobConfig(payload: TaskExecutionRollupJobConfigPayload): Promise<TaskExecutionRollupJobConfigResponse>;
  getSessionSummaries(
    sessionId: string,
    options?: { limit?: number; offset?: number },
  ): Promise<SessionSummariesListResponse>;
  deleteSessionSummary(sessionId: string, summaryId: string): Promise<{ success?: boolean }>;
  clearSessionSummaries(sessionId: string): Promise<{ success?: boolean }>;
  register(data: RegisterPayload): Promise<AuthResponse>;
  login(data: RegisterPayload): Promise<AuthResponse>;
  getMe(): Promise<MeResponse>;
  stopChat(sessionId: string, options?: { useResponses?: boolean }): Promise<StopChatResponse>;
  getUserSettings(userId?: string): Promise<UserSettingsResponse>;
  updateUserSettings(userId: string, settings: Record<string, unknown>): Promise<UserSettingsResponse>;
}

export const runtimeFacade: RuntimeFacade & ThisType<ApiClient> = {
  async submitRuntimeGuidance(payload) {
    return streamApi.submitRuntimeGuidance(this.getRequestFn(), payload);
  },
  async getTaskManagerTasks(sessionId, options) {
    return tasksApi.getTaskManagerTasks(this.getRequestFn(), sessionId, options);
  },
  async updateTaskManagerTask(sessionId, taskId, payload) {
    return tasksApi.updateTaskManagerTask(this.getRequestFn(), sessionId, taskId, payload);
  },
  async confirmTaskManagerTask(sessionId, taskId, payload) {
    return tasksApi.confirmTaskManagerTask(this.getRequestFn(), sessionId, taskId, payload);
  },
  async pauseTaskManagerTask(sessionId, taskId, payload) {
    return tasksApi.pauseTaskManagerTask(this.getRequestFn(), sessionId, taskId, payload);
  },
  async resumeTaskManagerTask(sessionId, taskId, payload) {
    return tasksApi.resumeTaskManagerTask(this.getRequestFn(), sessionId, taskId, payload);
  },
  async completeTaskManagerTask(sessionId, taskId) {
    return tasksApi.completeTaskManagerTask(this.getRequestFn(), sessionId, taskId);
  },
  async deleteTaskManagerTask(sessionId, taskId) {
    return tasksApi.deleteTaskManagerTask(this.getRequestFn(), sessionId, taskId);
  },
  async submitTaskReviewDecision(reviewId, payload) {
    return tasksApi.submitTaskReviewDecision(this.getRequestFn(), reviewId, payload);
  },
  async getImConversationActionRequests(conversationId) {
    return tasksApi.getImConversationActionRequests(this.getRequestFn(), conversationId);
  },
  async getImConversations() {
    return tasksApi.getImConversations(this.getRequestFn());
  },
  async getImWsMeta() {
    return tasksApi.getImWsMeta(this.getRequestFn());
  },
  async createImConversation(payload) {
    return tasksApi.createImConversation(this.getRequestFn(), payload);
  },
  async markImConversationRead(conversationId) {
    return tasksApi.markImConversationRead(this.getRequestFn(), conversationId);
  },
  async getImConversationMessages(conversationId, options) {
    return tasksApi.getImConversationMessages(this.getRequestFn(), conversationId, options);
  },
  async createImConversationMessage(conversationId, payload) {
    return tasksApi.createImConversationMessage(this.getRequestFn(), conversationId, payload);
  },
  async getImConversationRuns(conversationId) {
    return tasksApi.getImConversationRuns(this.getRequestFn(), conversationId);
  },
  async submitImActionRequest(actionRequestId, payload) {
    return tasksApi.submitImActionRequest(this.getRequestFn(), actionRequestId, payload);
  },
  async getPendingUiPrompts(sessionId, options) {
    return tasksApi.getPendingUiPrompts(this.getRequestFn(), sessionId, options);
  },
  async getUiPromptHistory(sessionId, options) {
    return tasksApi.getUiPromptHistory(this.getRequestFn(), sessionId, options);
  },
  async submitUiPromptResponse(promptId, payload) {
    return tasksApi.submitUiPromptResponse(this.getRequestFn(), promptId, payload);
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
  async getSessionSummaryJobConfig(userId) {
    return summaryApi.getSessionSummaryJobConfig(this.getRequestFn(), userId);
  },
  async updateSessionSummaryJobConfig(payload) {
    return summaryApi.updateSessionSummaryJobConfig(this.getRequestFn(), payload);
  },
  async patchSessionSummaryJobConfig(payload) {
    return summaryApi.patchSessionSummaryJobConfig(this.getRequestFn(), payload);
  },
  async getTaskExecutionSummaryJobConfig(userId) {
    return summaryApi.getTaskExecutionSummaryJobConfig(this.getRequestFn(), userId);
  },
  async updateTaskExecutionSummaryJobConfig(payload) {
    return summaryApi.updateTaskExecutionSummaryJobConfig(this.getRequestFn(), payload);
  },
  async patchTaskExecutionSummaryJobConfig(payload) {
    return summaryApi.patchTaskExecutionSummaryJobConfig(this.getRequestFn(), payload);
  },
  async getTaskExecutionRollupJobConfig(userId) {
    return summaryApi.getTaskExecutionRollupJobConfig(this.getRequestFn(), userId);
  },
  async updateTaskExecutionRollupJobConfig(payload) {
    return summaryApi.updateTaskExecutionRollupJobConfig(this.getRequestFn(), payload);
  },
  async patchTaskExecutionRollupJobConfig(payload) {
    return summaryApi.patchTaskExecutionRollupJobConfig(this.getRequestFn(), payload);
  },
  async getSessionSummaries(sessionId, options) {
    return summaryApi.getSessionSummaries(this.getRequestFn(), sessionId, options);
  },
  async deleteSessionSummary(sessionId, summaryId) {
    return summaryApi.deleteSessionSummary(this.getRequestFn(), sessionId, summaryId);
  },
  async clearSessionSummaries(sessionId) {
    return summaryApi.clearSessionSummaries(this.getRequestFn(), sessionId);
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
  async stopChat(sessionId, options) {
    return streamApi.stopChat(this.getRequestFn(), sessionId, options);
  },
  async getUserSettings(userId) {
    return accountApi.getUserSettings(this.getRequestFn(), userId);
  },
  async updateUserSettings(userId, settings) {
    return accountApi.updateUserSettings(this.getRequestFn(), userId, settings);
  },
};

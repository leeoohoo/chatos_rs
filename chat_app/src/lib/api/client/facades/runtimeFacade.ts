import * as accountApi from '../account';
import * as notepadApi from '../notepad';
import * as streamApi from '../stream';
import * as summaryApi from '../summary';
import * as tasksApi from '../tasks';
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
  RuntimeGuidanceSubmitPayload,
  RuntimeGuidanceSubmitResponse,
  SessionSummariesListResponse,
  SessionSummaryJobConfigPayload,
  SessionSummaryJobConfigResponse,
  StopChatResponse,
  StreamChatAttachmentPayload,
  StreamChatModelConfigPayload,
  StreamChatOptions,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  TaskReviewDecisionPayload,
  UiPromptItemResponse,
  UiPromptResponsePayload,
  UserSettingsResponse,
} from '../types';
import type ApiClient from '../../client';

export interface RuntimeFacade {
  streamChat(
    conversationId: string,
    content: string,
    modelConfig: StreamChatModelConfigPayload,
    userId?: string,
    attachments?: StreamChatAttachmentPayload[],
    reasoningEnabled?: boolean,
    options?: StreamChatOptions,
  ): Promise<ReadableStream>;
  submitRuntimeGuidance(payload: RuntimeGuidanceSubmitPayload): Promise<RuntimeGuidanceSubmitResponse>;
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
  submitTaskReviewDecision(
    reviewId: string,
    payload: TaskReviewDecisionPayload,
  ): Promise<{ success?: boolean; status?: string }>;
  getPendingUiPrompts(conversationId: string, options?: { limit?: number }): Promise<UiPromptItemResponse[]>;
  getUiPromptHistory(
    conversationId: string,
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
  getConversationSummaryJobConfig(userId?: string): Promise<SessionSummaryJobConfigResponse>;
  updateConversationSummaryJobConfig(payload: SessionSummaryJobConfigPayload): Promise<SessionSummaryJobConfigResponse>;
  patchConversationSummaryJobConfig(payload: SessionSummaryJobConfigPayload): Promise<SessionSummaryJobConfigResponse>;
  getConversationSummaries(
    conversationId: string,
    options?: { limit?: number; offset?: number },
  ): Promise<SessionSummariesListResponse>;
  deleteConversationSummary(conversationId: string, summaryId: string): Promise<{ success?: boolean }>;
  clearConversationSummaries(conversationId: string): Promise<{ success?: boolean }>;
  register(data: RegisterPayload): Promise<AuthResponse>;
  login(data: RegisterPayload): Promise<AuthResponse>;
  getMe(): Promise<MeResponse>;
  stopChat(conversationId: string, options?: { useResponses?: boolean }): Promise<StopChatResponse>;
  getUserSettings(userId?: string): Promise<UserSettingsResponse>;
  updateUserSettings(userId: string, settings: Record<string, unknown>): Promise<UserSettingsResponse>;
}

export const runtimeFacade: RuntimeFacade & ThisType<ApiClient> = {
  async streamChat(conversationId, content, modelConfig, userId, attachments, reasoningEnabled, options) {
    return streamApi.streamChat(
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
  async submitRuntimeGuidance(payload) {
    return streamApi.submitRuntimeGuidance(this.getRequestFn(), payload);
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
  async submitTaskReviewDecision(reviewId, payload) {
    return tasksApi.submitTaskReviewDecision(this.getRequestFn(), reviewId, payload);
  },
  async getPendingUiPrompts(conversationId, options) {
    return tasksApi.getPendingUiPrompts(this.getRequestFn(), conversationId, options);
  },
  async getUiPromptHistory(conversationId, options) {
    return tasksApi.getUiPromptHistory(this.getRequestFn(), conversationId, options);
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
  async getConversationSummaryJobConfig(userId) {
    return summaryApi.getConversationSummaryJobConfig(this.getRequestFn(), userId);
  },
  async updateConversationSummaryJobConfig(payload) {
    return summaryApi.updateConversationSummaryJobConfig(this.getRequestFn(), payload);
  },
  async patchConversationSummaryJobConfig(payload) {
    return summaryApi.patchConversationSummaryJobConfig(this.getRequestFn(), payload);
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
  async register(data) {
    return accountApi.register(this.getRequestFn(), data);
  },
  async login(data) {
    return accountApi.login(this.getRequestFn(), data);
  },
  async getMe() {
    return accountApi.getMe(this.getRequestFn());
  },
  async stopChat(conversationId, options) {
    return streamApi.stopChat(this.getRequestFn(), conversationId, options);
  },
  async getUserSettings(userId) {
    return accountApi.getUserSettings(this.getRequestFn(), userId);
  },
  async updateUserSettings(userId, settings) {
    return accountApi.updateUserSettings(this.getRequestFn(), userId, settings);
  },
};

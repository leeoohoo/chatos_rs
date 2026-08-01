// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as workspaceApi from '../../workspace';
import type {
  AnalyzeProjectRuntimeEnvironmentPayload,
  DeleteSuccessResponse,
  PagingOptions,
  ProjectContactLockResponse,
  ProjectContactLinkResponse,
  ProjectPlanOptions,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementConfirmResponse,
  ProjectRequirementWorkItemsOptions,
  ProjectRequirementWorkItemsResponse,
  ProjectRequirementExecuteResponse,
  ProjectRequirementExecutionPlanResponse,
  ProjectRequirementDispatchResponse,
  ProjectRequirementStopResponse,
  ProjectRuntimeEnvironmentResponse,
  ProjectRuntimeEnvironmentProgressResponse,
  ProjectRunEnvironmentResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
  ProjectRunStateResponse,
  UpdateProjectRuntimeEnvironmentSettingsPayload,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceProjectFacade {
  listProjects(userId?: string): Promise<ProjectResponse[]>;
  createCloudProject(data: FormData): Promise<ProjectResponse>;
  updateProject(id: string, data: { name?: string; git_url?: string; description?: string }): Promise<ProjectResponse>;
  deleteProject(id: string): Promise<DeleteSuccessResponse>;
  getProject(id: string): Promise<ProjectResponse>;
  getProjectRuntimeEnvironment(projectId: string): Promise<ProjectRuntimeEnvironmentResponse>;
  updateProjectRuntimeEnvironmentSettings(
    projectId: string,
    data: UpdateProjectRuntimeEnvironmentSettingsPayload,
  ): Promise<ProjectRuntimeEnvironmentResponse>;
  analyzeProjectRuntimeEnvironment(
    projectId: string,
    data?: AnalyzeProjectRuntimeEnvironmentPayload,
  ): Promise<ProjectRuntimeEnvironmentResponse>;
  generateProjectRuntimeEnvironmentImage(
    projectId: string,
    imageRecordId: string,
  ): Promise<ProjectRuntimeEnvironmentResponse>;
  getProjectRuntimeEnvironmentProgress(
    projectId: string,
  ): Promise<ProjectRuntimeEnvironmentProgressResponse>;
  getProjectPlan(projectId: string, options?: ProjectPlanOptions): Promise<ProjectPlanResponse>;
  listProjectRequirementWorkItems(
    projectId: string,
    requirementId: string,
    options?: ProjectRequirementWorkItemsOptions,
  ): Promise<ProjectRequirementWorkItemsResponse>;
  listProjectRequirementDocuments(
    projectId: string,
    requirementId: string,
  ): Promise<ProjectRequirementDocumentResponse[]>;
  executeProjectRequirement(
    projectId: string,
    requirementId: string,
    data?: {
      contact_id?: string;
      model_config_id?: string;
      modelConfigId?: string;
      include_prerequisite_dependents?: boolean;
      includePrerequisiteDependents?: boolean;
      planning_feedback?: string;
      planningFeedback?: string;
      replaces_execution_group_id?: string;
      replacesExecutionGroupId?: string;
      replaces_conversation_id?: string;
      replacesConversationId?: string;
    },
  ): Promise<ProjectRequirementExecuteResponse>;
  getProjectRequirementExecutionPlan(
    projectId: string,
    requirementId: string,
    identity?: { conversationId?: string; executionGroupId?: string },
  ): Promise<ProjectRequirementExecutionPlanResponse>;
  confirmProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data: {
      execution_group_id: string;
      conversation_id: string;
      contact_id?: string;
    },
  ): Promise<ProjectRequirementConfirmResponse>;
  pauseProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data: {
      execution_group_id: string;
      conversation_id: string;
      contact_id?: string;
    },
  ): Promise<ProjectRequirementDispatchResponse>;
  resumeProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data: {
      execution_group_id: string;
      conversation_id: string;
      contact_id?: string;
    },
  ): Promise<ProjectRequirementDispatchResponse>;
  stopProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data?: {
      contact_id?: string;
      execution_group_id?: string;
      conversation_id?: string;
      discard_tasks?: boolean;
    },
  ): Promise<ProjectRequirementStopResponse>;
  rerunProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data: {
      execution_group_id: string;
      conversation_id: string;
      contact_id?: string;
    },
  ): Promise<ProjectRequirementExecuteResponse>;
  analyzeProjectRun(projectId: string): Promise<ProjectRunCatalogResponse>;
  getProjectRunCatalog(projectId: string): Promise<ProjectRunCatalogResponse>;
  getProjectRunState(projectId: string): Promise<ProjectRunStateResponse>;
  getProjectRunEnvironment(projectId: string): Promise<ProjectRunEnvironmentResponse>;
  updateProjectRunEnvironment(
    projectId: string,
    data: {
      selected_toolchains?: Record<string, string>;
      custom_toolchains?: Record<string, { kind?: string; label?: string; path?: string }>;
      env_vars?: Record<string, string>;
      terminal_ui_enabled?: boolean;
    },
  ): Promise<ProjectRunEnvironmentResponse>;
  executeProjectRun(
    projectId: string,
    data: {
      target_id?: string;
      cwd?: string;
      command?: string;
      create_if_missing?: boolean;
      terminal_id?: string;
    },
  ): Promise<ProjectRunExecuteResponse>;
  setProjectRunDefault(projectId: string, targetId: string): Promise<ProjectRunCatalogResponse>;
  listProjectContacts(projectId: string, paging?: PagingOptions): Promise<ProjectContactLinkResponse[]>;
  getProjectContactLock(projectId: string): Promise<ProjectContactLockResponse>;
  addProjectContact(projectId: string, data: { contact_id: string }): Promise<ProjectContactLinkResponse>;
  removeProjectContact(projectId: string, contactId: string): Promise<DeleteSuccessResponse>;
}

export const workspaceProjectFacade: WorkspaceProjectFacade & ThisType<ApiClient> = {
  async listProjects(userId) {
    return workspaceApi.listProjects(this.getRequestFn(), userId);
  },
  async createCloudProject(data) {
    return workspaceApi.createCloudProject(this.getRequestFn(), data);
  },
  async updateProject(id, data) {
    return workspaceApi.updateProject(this.getRequestFn(), id, data);
  },
  async deleteProject(id) {
    return workspaceApi.deleteProject(this.getRequestFn(), id);
  },
  async getProject(id) {
    return workspaceApi.getProject(this.getRequestFn(), id);
  },
  async getProjectRuntimeEnvironment(projectId) {
    return workspaceApi.getProjectRuntimeEnvironment(this.getRequestFn(), projectId);
  },
  async updateProjectRuntimeEnvironmentSettings(projectId, data) {
    return workspaceApi.updateProjectRuntimeEnvironmentSettings(this.getRequestFn(), projectId, data);
  },
  async analyzeProjectRuntimeEnvironment(projectId, data = {}) {
    return workspaceApi.analyzeProjectRuntimeEnvironment(this.getRequestFn(), projectId, data);
  },
  async generateProjectRuntimeEnvironmentImage(projectId, imageRecordId) {
    return workspaceApi.generateProjectRuntimeEnvironmentImage(
      this.getRequestFn(),
      projectId,
      imageRecordId,
    );
  },
  async getProjectRuntimeEnvironmentProgress(projectId) {
    return workspaceApi.getProjectRuntimeEnvironmentProgress(this.getRequestFn(), projectId);
  },
  async getProjectPlan(projectId, options) {
    return workspaceApi.getProjectPlan(this.getRequestFn(), projectId, options);
  },
  async listProjectRequirementWorkItems(projectId, requirementId, options) {
    return workspaceApi.listProjectRequirementWorkItems(
      this.getRequestFn(),
      projectId,
      requirementId,
      options,
    );
  },
  async listProjectRequirementDocuments(projectId, requirementId) {
    return workspaceApi.listProjectRequirementDocuments(
      this.getRequestFn(),
      projectId,
      requirementId,
    );
  },
  async executeProjectRequirement(projectId, requirementId, data) {
    return workspaceApi.executeProjectRequirement(this.getRequestFn(), projectId, requirementId, data);
  },
  async getProjectRequirementExecutionPlan(projectId, requirementId, identity) {
    return workspaceApi.getProjectRequirementExecutionPlan(
      this.getRequestFn(),
      projectId,
      requirementId,
      identity,
    );
  },
  async confirmProjectRequirementExecution(projectId, requirementId, data) {
    return workspaceApi.confirmProjectRequirementExecution(
      this.getRequestFn(),
      projectId,
      requirementId,
      data,
    );
  },
  async pauseProjectRequirementExecution(projectId, requirementId, data) {
    return workspaceApi.pauseProjectRequirementExecution(
      this.getRequestFn(),
      projectId,
      requirementId,
      data,
    );
  },
  async resumeProjectRequirementExecution(projectId, requirementId, data) {
    return workspaceApi.resumeProjectRequirementExecution(
      this.getRequestFn(),
      projectId,
      requirementId,
      data,
    );
  },
  async stopProjectRequirementExecution(projectId, requirementId, data) {
    return workspaceApi.stopProjectRequirementExecution(this.getRequestFn(), projectId, requirementId, data);
  },
  async rerunProjectRequirementExecution(projectId, requirementId, data) {
    return workspaceApi.rerunProjectRequirementExecution(
      this.getRequestFn(),
      projectId,
      requirementId,
      data,
    );
  },
  async analyzeProjectRun(projectId) {
    return workspaceApi.analyzeProjectRun(this.getRequestFn(), projectId);
  },
  async getProjectRunCatalog(projectId) {
    return workspaceApi.getProjectRunCatalog(this.getRequestFn(), projectId);
  },
  async getProjectRunState(projectId) {
    return workspaceApi.getProjectRunState(this.getRequestFn(), projectId);
  },
  async getProjectRunEnvironment(projectId) {
    return workspaceApi.getProjectRunEnvironment(this.getRequestFn(), projectId);
  },
  async updateProjectRunEnvironment(projectId, data) {
    return workspaceApi.updateProjectRunEnvironment(this.getRequestFn(), projectId, data);
  },
  async executeProjectRun(projectId, data) {
    return workspaceApi.executeProjectRun(this.getRequestFn(), projectId, data);
  },
  async setProjectRunDefault(projectId, targetId) {
    return workspaceApi.setProjectRunDefault(this.getRequestFn(), projectId, targetId);
  },
  async listProjectContacts(projectId, paging) {
    return workspaceApi.listProjectContacts(
      this.getRequestFn(),
      projectId,
      paging,
    );
  },
  async getProjectContactLock(projectId) {
    return workspaceApi.getProjectContactLock(
      this.getRequestFn(),
      projectId,
    );
  },
  async addProjectContact(projectId, data) {
    return workspaceApi.addProjectContact(
      this.getRequestFn(),
      projectId,
      data,
    );
  },
  async removeProjectContact(projectId, contactId) {
    return workspaceApi.removeProjectContact(
      this.getRequestFn(),
      projectId,
      contactId,
    );
  },
};

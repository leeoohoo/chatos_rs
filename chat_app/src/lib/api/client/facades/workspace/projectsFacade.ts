// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as workspaceApi from '../../workspace';
import type {
  DeleteSuccessResponse,
  PagingOptions,
  ProjectContactLockResponse,
  ProjectContactLinkResponse,
  ProjectPlanOptions,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementWorkItemsOptions,
  ProjectRequirementWorkItemsResponse,
  ProjectRequirementExecuteResponse,
  ProjectRequirementStopResponse,
  ProjectRunEnvironmentResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
  ProjectRunStateResponse,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceProjectFacade {
  listProjects(userId?: string): Promise<ProjectResponse[]>;
  createProject(data: { name: string; root_path: string; git_url?: string; description?: string; user_id?: string }): Promise<ProjectResponse>;
  updateProject(id: string, data: { name?: string; root_path?: string; git_url?: string; description?: string }): Promise<ProjectResponse>;
  deleteProject(id: string): Promise<DeleteSuccessResponse>;
  getProject(id: string): Promise<ProjectResponse>;
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
    data?: { contact_id?: string; include_prerequisite_dependents?: boolean; includePrerequisiteDependents?: boolean },
  ): Promise<ProjectRequirementExecuteResponse>;
  stopProjectRequirementExecution(
    projectId: string,
    requirementId: string,
    data?: { contact_id?: string },
  ): Promise<ProjectRequirementStopResponse>;
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
  async createProject(data) {
    return workspaceApi.createProject(this.getRequestFn(), data);
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
  async stopProjectRequirementExecution(projectId, requirementId, data) {
    return workspaceApi.stopProjectRequirementExecution(this.getRequestFn(), projectId, requirementId, data);
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
    return workspaceApi.listProjectContacts(this.getRequestFn(), projectId, paging);
  },
  async getProjectContactLock(projectId) {
    return workspaceApi.getProjectContactLock(this.getRequestFn(), projectId);
  },
  async addProjectContact(projectId, data) {
    return workspaceApi.addProjectContact(this.getRequestFn(), projectId, data);
  },
  async removeProjectContact(projectId, contactId) {
    return workspaceApi.removeProjectContact(this.getRequestFn(), projectId, contactId);
  },
};

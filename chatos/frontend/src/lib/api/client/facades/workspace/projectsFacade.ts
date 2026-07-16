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
import { localRuntimeBridgeAvailable } from '../../../localRuntime';

const requireDesktopProjectCreation = (): void => {
  if (!localRuntimeBridgeAvailable()) {
    throw new Error('项目只能在 Chat OS 桌面客户端中创建');
  }
};

const projectResponseUsesLocalRuntime = (project: ProjectResponse): boolean => {
  const executionPlane = String(project.execution_plane || project.executionPlane || '').trim();
  const sourceType = String(project.source_type || project.sourceType || '').trim();
  const rootPath = String(project.root_path || project.rootPath || '').trim();
  return executionPlane === 'local_connector'
    || sourceType === 'local_connector'
    || sourceType === 'local'
    || rootPath.startsWith('local://connector/');
};

const cloudProjectCache = new WeakMap<object, ProjectResponse[]>();
const DESKTOP_CLOUD_PROJECT_WAIT_MS = 800;

const withinDesktopCloudWaitBudget = async (
  pending: Promise<ProjectResponse[]>,
  fallback: ProjectResponse[],
): Promise<ProjectResponse[]> => new Promise((resolve) => {
  let settled = false;
  const finish = (projects: ProjectResponse[]) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    resolve(projects);
  };
  const timer = setTimeout(() => finish(fallback), DESKTOP_CLOUD_PROJECT_WAIT_MS);
  void pending.then(finish);
});

export interface WorkspaceProjectFacade {
  listProjects(userId?: string): Promise<ProjectResponse[]>;
  createProject(data: { name: string; root_path: string; git_url?: string; description?: string; user_id?: string }): Promise<ProjectResponse>;
  createCloudProject(data: FormData): Promise<ProjectResponse>;
  updateProject(id: string, data: { name?: string; root_path?: string; git_url?: string; description?: string }): Promise<ProjectResponse>;
  deleteProject(id: string): Promise<DeleteSuccessResponse>;
  getProject(id: string): Promise<ProjectResponse>;
  getProjectRuntimeEnvironment(projectId: string): Promise<ProjectRuntimeEnvironmentResponse>;
  updateProjectRuntimeEnvironmentSettings(
    projectId: string,
    data: UpdateProjectRuntimeEnvironmentSettingsPayload,
  ): Promise<ProjectRuntimeEnvironmentResponse>;
  analyzeProjectRuntimeEnvironment(projectId: string): Promise<ProjectRuntimeEnvironmentResponse>;
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
    if (!localRuntimeBridgeAvailable()) {
      const cloudProjects = await workspaceApi.listProjects(this.getRequestFn(), userId);
      return cloudProjects.filter((project) => !projectResponseUsesLocalRuntime(project));
    }

    const localProjects = await this.getLocalRuntimeClient().listProjects();
    localProjects.forEach((project) => this.registerLocalProjectExecution(project.id));
    const cachedCloudProjects = cloudProjectCache.get(this) || [];
    const cloudRequest = workspaceApi.listProjects(this.getRequestFn(), userId)
      .then((projects) => projects.filter((project) => !projectResponseUsesLocalRuntime(project)))
      .then((projects) => {
        cloudProjectCache.set(this, projects);
        return projects;
      })
      .catch((error) => {
        console.warn('Cloud projects are temporarily unavailable; keeping local projects visible.', error);
        return cachedCloudProjects;
      });
    const cloudOnly = await withinDesktopCloudWaitBudget(cloudRequest, cachedCloudProjects);
    return [...localProjects, ...cloudOnly];
  },
  async createProject(data) {
    requireDesktopProjectCreation();
    return workspaceApi.createProject(this.getRequestFn(), data);
  },
  async createCloudProject(data) {
    return workspaceApi.createCloudProject(this.getRequestFn(), data);
  },
  async updateProject(id, data) {
    if (this.projectUsesLocalRuntime(id)) {
      return this.getLocalRuntimeClient().updateProject(id, data);
    }
    return workspaceApi.updateProject(this.getRequestFn(), id, data);
  },
  async deleteProject(id) {
    if (this.projectUsesLocalRuntime(id)) {
      return this.getLocalRuntimeClient().deleteProject(id);
    }
    return workspaceApi.deleteProject(this.getRequestFn(), id);
  },
  async getProject(id) {
    if (this.projectUsesLocalRuntime(id)) {
      return this.getLocalRuntimeClient().getProject(id);
    }
    return workspaceApi.getProject(this.getRequestFn(), id);
  },
  async getProjectRuntimeEnvironment(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getProjectRuntimeEnvironment(projectId);
    }
    return workspaceApi.getProjectRuntimeEnvironment(this.getRequestFn(), projectId);
  },
  async updateProjectRuntimeEnvironmentSettings(projectId, data) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().updateProjectRuntimeEnvironmentSettings(projectId, data);
    }
    return workspaceApi.updateProjectRuntimeEnvironmentSettings(this.getRequestFn(), projectId, data);
  },
  async analyzeProjectRuntimeEnvironment(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().analyzeProjectRuntimeEnvironment(projectId);
    }
    return workspaceApi.analyzeProjectRuntimeEnvironment(this.getRequestFn(), projectId);
  },
  async generateProjectRuntimeEnvironmentImage(projectId, imageRecordId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      throw new Error('本地项目镜像必须由本地客户端生成');
    }
    return workspaceApi.generateProjectRuntimeEnvironmentImage(
      this.getRequestFn(),
      projectId,
      imageRecordId,
    );
  },
  async getProjectRuntimeEnvironmentProgress(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getProjectRuntimeEnvironmentProgress(projectId);
    }
    return workspaceApi.getProjectRuntimeEnvironmentProgress(this.getRequestFn(), projectId);
  },
  async getProjectPlan(projectId, options) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getProjectPlan(projectId, options);
    }
    return workspaceApi.getProjectPlan(this.getRequestFn(), projectId, options);
  },
  async listProjectRequirementWorkItems(projectId, requirementId, options) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().listProjectRequirementWorkItems(
        projectId,
        requirementId,
        options,
      );
    }
    return workspaceApi.listProjectRequirementWorkItems(
      this.getRequestFn(),
      projectId,
      requirementId,
      options,
    );
  },
  async listProjectRequirementDocuments(projectId, requirementId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().listProjectRequirementDocuments(
        projectId,
        requirementId,
      );
    }
    return workspaceApi.listProjectRequirementDocuments(
      this.getRequestFn(),
      projectId,
      requirementId,
    );
  },
  async executeProjectRequirement(projectId, requirementId, data) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().executeProjectRequirement(
        projectId,
        requirementId,
        data,
      );
    }
    return workspaceApi.executeProjectRequirement(this.getRequestFn(), projectId, requirementId, data);
  },
  async stopProjectRequirementExecution(projectId, requirementId, data) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().stopProjectRequirementExecution(
        projectId,
        requirementId,
        data,
      );
    }
    return workspaceApi.stopProjectRequirementExecution(this.getRequestFn(), projectId, requirementId, data);
  },
  async analyzeProjectRun(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().analyzeProjectRun(projectId);
    }
    return workspaceApi.analyzeProjectRun(this.getRequestFn(), projectId);
  },
  async getProjectRunCatalog(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getProjectRunCatalog(projectId);
    }
    return workspaceApi.getProjectRunCatalog(this.getRequestFn(), projectId);
  },
  async getProjectRunState(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getProjectRunState(projectId);
    }
    return workspaceApi.getProjectRunState(this.getRequestFn(), projectId);
  },
  async getProjectRunEnvironment(projectId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getProjectRunEnvironment(projectId);
    }
    return workspaceApi.getProjectRunEnvironment(this.getRequestFn(), projectId);
  },
  async updateProjectRunEnvironment(projectId, data) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().updateProjectRunEnvironment(projectId, data);
    }
    return workspaceApi.updateProjectRunEnvironment(this.getRequestFn(), projectId, data);
  },
  async executeProjectRun(projectId, data) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().executeProjectRun(projectId, data);
    }
    return workspaceApi.executeProjectRun(this.getRequestFn(), projectId, data);
  },
  async setProjectRunDefault(projectId, targetId) {
    if (this.projectUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().setProjectRunDefault(projectId, targetId);
    }
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

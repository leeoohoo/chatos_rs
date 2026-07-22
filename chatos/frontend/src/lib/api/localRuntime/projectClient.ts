// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../../types';
import type {
  CreateLocalConnectorProjectRequest,
  DeleteSuccessResponse,
  ProjectPlanOptions,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementExecuteResponse,
  ProjectRequirementStopResponse,
  ProjectRequirementWorkItemsOptions,
  ProjectRequirementWorkItemsResponse,
  ProjectResponse,
  ProjectRuntimeEnvironmentProgressResponse,
  ProjectRuntimeEnvironmentResponse,
  UpdateProjectRuntimeEnvironmentSettingsPayload,
} from '../client/types';
import { requestLocalRuntime } from './bridge';
import { getLocalProjectPlan, listLocalProjectRequirementDocuments, listLocalProjectRequirementWorkItems } from './projectManagement';
import { parseLocalConnectorProjectRoot } from './projectRoot';
import {
  analyzeLocalProjectRun,
  executeLocalProjectRun,
  getLocalProjectRunCatalog,
  getLocalProjectRunEnvironment,
  getLocalProjectRunState,
  setLocalProjectRunDefault,
  updateLocalProjectRunEnvironment,
} from './projectRuns';
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
import { LocalRuntimeResourceClient } from './resourceClient';
import {
  analyzeLocalProjectRuntimeEnvironment,
  getLocalProjectRuntimeEnvironment,
  getLocalProjectRuntimeEnvironmentProgress,
  startLocalProjectRuntimeEnvironment,
  updateLocalProjectRuntimeEnvironmentSettings,
} from './runtimeEnvironment';
import type { LocalRuntimeProjectRecord } from './types';

export class LocalRuntimeProjectClient extends LocalRuntimeResourceClient {
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
      model_config_id?: string;
      modelConfigId?: string;
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

  async startProjectRuntimeEnvironment(
    projectId: string,
  ): Promise<ProjectRuntimeEnvironmentResponse> {
    return startLocalProjectRuntimeEnvironment(projectId);
  }

  async getProjectRuntimeEnvironmentProgress(
    projectId: string,
  ): Promise<ProjectRuntimeEnvironmentProgressResponse> {
    return getLocalProjectRuntimeEnvironmentProgress(projectId);
  }

  analyzeProjectRun(projectId: string) {
    return analyzeLocalProjectRun(projectId);
  }

  getProjectRunCatalog(projectId: string) {
    return getLocalProjectRunCatalog(projectId);
  }

  getProjectRunState(projectId: string) {
    return getLocalProjectRunState(projectId);
  }

  getProjectRunEnvironment(projectId: string) {
    return getLocalProjectRunEnvironment(projectId);
  }

  updateProjectRunEnvironment(
    projectId: string,
    data: {
      selected_toolchains?: Record<string, string>;
      custom_toolchains?: Record<string, { kind?: string; label?: string; path?: string }>;
      env_vars?: Record<string, string>;
      terminal_ui_enabled?: boolean;
    },
  ) {
    return updateLocalProjectRunEnvironment(projectId, data);
  }

  executeProjectRun(
    projectId: string,
    data: {
      target_id?: string;
      cwd?: string;
      command?: string;
      create_if_missing?: boolean;
      terminal_id?: string;
    },
  ) {
    return executeLocalProjectRun(projectId, data);
  }

  setProjectRunDefault(projectId: string, targetId: string) {
    return setLocalProjectRunDefault(projectId, targetId);
  }
}

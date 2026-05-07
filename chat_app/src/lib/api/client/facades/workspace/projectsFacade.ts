import * as workspaceApi from '../../workspace';
import type {
  DeleteSuccessResponse,
  PagingOptions,
  ProjectChangeConfirmResponse,
  ProjectChangeLogResponse,
  ProjectChangeSummaryResponse,
  ProjectContactLinkResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceProjectFacade {
  listProjects(userId?: string): Promise<ProjectResponse[]>;
  createProject(data: { name: string; root_path: string; description?: string; user_id?: string }): Promise<ProjectResponse>;
  updateProject(id: string, data: { name?: string; root_path?: string; description?: string }): Promise<ProjectResponse>;
  deleteProject(id: string): Promise<DeleteSuccessResponse>;
  getProject(id: string): Promise<ProjectResponse>;
  analyzeProjectRun(projectId: string): Promise<ProjectRunCatalogResponse>;
  getProjectRunCatalog(projectId: string): Promise<ProjectRunCatalogResponse>;
  executeProjectRun(
    projectId: string,
    data: { target_id?: string; cwd?: string; command?: string; create_if_missing?: boolean },
  ): Promise<ProjectRunExecuteResponse>;
  setProjectRunDefault(projectId: string, targetId: string): Promise<ProjectRunCatalogResponse>;
  listProjectContacts(projectId: string, paging?: PagingOptions): Promise<ProjectContactLinkResponse[]>;
  addProjectContact(projectId: string, data: { contact_id: string }): Promise<ProjectContactLinkResponse>;
  removeProjectContact(projectId: string, contactId: string): Promise<DeleteSuccessResponse>;
  listProjectChangeLogs(
    projectId: string,
    params?: { path?: string; limit?: number; offset?: number },
  ): Promise<ProjectChangeLogResponse[]>;
  getProjectChangeSummary(projectId: string): Promise<ProjectChangeSummaryResponse>;
  confirmProjectChanges(
    projectId: string,
    payload: { mode?: 'all' | 'paths' | 'change_ids'; paths?: string[]; change_ids?: string[] },
  ): Promise<ProjectChangeConfirmResponse>;
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
  async analyzeProjectRun(projectId) {
    return workspaceApi.analyzeProjectRun(this.getRequestFn(), projectId);
  },
  async getProjectRunCatalog(projectId) {
    return workspaceApi.getProjectRunCatalog(this.getRequestFn(), projectId);
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
  async addProjectContact(projectId, data) {
    return workspaceApi.addProjectContact(this.getRequestFn(), projectId, data);
  },
  async removeProjectContact(projectId, contactId) {
    return workspaceApi.removeProjectContact(this.getRequestFn(), projectId, contactId);
  },
  async listProjectChangeLogs(projectId, params) {
    return workspaceApi.listProjectChangeLogs(this.getRequestFn(), projectId, params);
  },
  async getProjectChangeSummary(projectId) {
    return workspaceApi.getProjectChangeSummary(this.getRequestFn(), projectId);
  },
  async confirmProjectChanges(projectId, payload) {
    return workspaceApi.confirmProjectChanges(this.getRequestFn(), projectId, payload);
  },
};

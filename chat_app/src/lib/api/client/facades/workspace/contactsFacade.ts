import * as workspaceApi from '../../workspace';
import type {
  ContactAgentRecallResponse,
  ContactCreateResponse,
  ContactProjectLinkResponse,
  ContactProjectMemoryResponse,
  ContactResponse,
  ContactTaskRunnerUpdatePayload,
  DeleteSuccessResponse,
  PagingOptions,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceContactFacade {
  getContacts(userId?: string, paging?: PagingOptions): Promise<ContactResponse[]>;
  getContact(contactId: string): Promise<ContactResponse>;
  createContact(data: { agent_id: string; agent_name_snapshot?: string; user_id?: string }): Promise<ContactCreateResponse>;
  deleteContact(contactId: string): Promise<DeleteSuccessResponse>;
  updateContactTaskRunnerConfig(contactId: string, data: ContactTaskRunnerUpdatePayload): Promise<ContactResponse>;
  getContactProjectMemories(
    contactId: string,
    projectId: string,
    paging?: PagingOptions,
  ): Promise<ContactProjectMemoryResponse[]>;
  getContactProjects(contactId: string, paging?: PagingOptions): Promise<ContactProjectLinkResponse[]>;
  getContactAgentRecalls(contactId: string, paging?: PagingOptions): Promise<ContactAgentRecallResponse[]>;
}

export const workspaceContactFacade: WorkspaceContactFacade & ThisType<ApiClient> = {
  async getContacts(userId, paging) {
    return workspaceApi.getContacts(this.getRequestFn(), userId, paging);
  },
  async getContact(contactId) {
    return workspaceApi.getContact(this.getRequestFn(), contactId);
  },
  async createContact(data) {
    return workspaceApi.createContact(this.getRequestFn(), data);
  },
  async deleteContact(contactId) {
    return workspaceApi.deleteContact(this.getRequestFn(), contactId);
  },
  async updateContactTaskRunnerConfig(contactId, data) {
    return workspaceApi.updateContactTaskRunnerConfig(this.getRequestFn(), contactId, data);
  },
  async getContactProjectMemories(contactId, projectId, paging) {
    return workspaceApi.getContactProjectMemories(this.getRequestFn(), contactId, projectId, paging);
  },
  async getContactProjects(contactId, paging) {
    return workspaceApi.getContactProjects(this.getRequestFn(), contactId, paging);
  },
  async getContactAgentRecalls(contactId, paging) {
    return workspaceApi.getContactAgentRecalls(this.getRequestFn(), contactId, paging);
  },
};

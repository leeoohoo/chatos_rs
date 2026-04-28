import { buildQuery } from '../shared';
import type {
  ContactAgentRecallResponse,
  ContactCreateResponse,
  ContactProjectLinkResponse,
  ContactProjectMemoryResponse,
  ContactResponse,
  DeleteSuccessResponse,
} from '../types';
import type { ApiRequestFn, ContactPaging } from './common';

export const getContacts = (
  request: ApiRequestFn,
  userId?: string,
  paging?: ContactPaging,
): Promise<ContactResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactResponse[]>(`/contacts${query}`);
};

export const createContact = (
  request: ApiRequestFn,
  data: { agent_id: string; agent_name_snapshot?: string; user_id?: string },
): Promise<ContactCreateResponse> => {
  return request<ContactCreateResponse>('/contacts', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const deleteContact = (
  request: ApiRequestFn,
  contactId: string,
): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/contacts/${contactId}`, {
    method: 'DELETE',
  });
};

export const getContactProjectMemories = (
  request: ApiRequestFn,
  contactId: string,
  projectId: string,
  paging?: ContactPaging,
): Promise<ContactProjectMemoryResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactProjectMemoryResponse[]>(
    `/contacts/${encodeURIComponent(contactId)}/project-memories/${encodeURIComponent(projectId)}${query}`,
  );
};

export const getContactProjects = (
  request: ApiRequestFn,
  contactId: string,
  paging?: ContactPaging,
): Promise<ContactProjectLinkResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactProjectLinkResponse[]>(
    `/contacts/${encodeURIComponent(contactId)}/projects${query}`,
  );
};

export const getContactAgentRecalls = (
  request: ApiRequestFn,
  contactId: string,
  paging?: ContactPaging,
): Promise<ContactAgentRecallResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactAgentRecallResponse[]>(
    `/contacts/${encodeURIComponent(contactId)}/agent-recalls${query}`,
  );
};

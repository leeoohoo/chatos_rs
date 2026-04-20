import type { MessageCreatePayload, SessionMessageResponse } from './types';
import type { ApiRequestFn } from './workspace';

export const createMessage = (
  request: ApiRequestFn,
  data: MessageCreatePayload,
): Promise<SessionMessageResponse> => {
  const requestData = {
    ...data,
    createdAt: data.createdAt ? data.createdAt.toISOString() : undefined,
  };
  return request<SessionMessageResponse>(`/conversations/${data.conversationId}/messages`, {
    method: 'POST',
    body: JSON.stringify(requestData),
  });
};

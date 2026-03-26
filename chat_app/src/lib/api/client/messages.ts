import type { MessageCreatePayload } from './types';
import type { ApiRequestFn } from './workspace';

export const createMessage = (
  request: ApiRequestFn,
  data: MessageCreatePayload,
): Promise<any> => {
  const requestData = {
    ...data,
    createdAt: data.createdAt ? data.createdAt.toISOString() : undefined,
  };
  return request<any>(`/sessions/${data.sessionId}/messages`, {
    method: 'POST',
    body: JSON.stringify(requestData),
  });
};

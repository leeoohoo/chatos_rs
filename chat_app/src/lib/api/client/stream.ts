import type {
  RuntimeGuidanceSubmitPayload,
  RuntimeGuidanceSubmitResponse,
  StopChatResponse,
} from './types';
import type { ApiRequestFn } from './workspace';

export const stopChat = (
  request: ApiRequestFn,
  sessionId: string,
  options?: { useResponses?: boolean }
): Promise<StopChatResponse> => {
  const useResponses = options?.useResponses === true;
  const path = useResponses ? '/agent_v3/chat/stop' : '/chat/stop';
  return request<StopChatResponse>(path, {
    method: 'POST',
    body: JSON.stringify({
      session_id: sessionId,
    }),
  });
};

export const submitRuntimeGuidance = (
  request: ApiRequestFn,
  payload: RuntimeGuidanceSubmitPayload,
): Promise<RuntimeGuidanceSubmitResponse> => {
  return request<RuntimeGuidanceSubmitResponse>('/agent_v3/chat/guide', {
    method: 'POST',
    body: JSON.stringify({
      session_id: payload.sessionId,
      turn_id: payload.turnId,
      content: payload.content,
      project_id: payload.projectId,
    }),
  });
};

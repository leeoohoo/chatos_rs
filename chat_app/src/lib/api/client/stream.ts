import type { ApiRequestFn } from './workspace';

export interface StreamApiContext {
  baseUrl: string;
  accessToken: string | null;
  applyRefreshedAccessToken: (response: Response) => void;
}

export const streamChat = async (
  context: StreamApiContext,
  sessionId: string,
  content: string,
  modelConfig: any,
  userId?: string,
  attachments?: any[],
  reasoningEnabled?: boolean,
  options?: { turnId?: string }
): Promise<ReadableStream> => {
  const useResponses = modelConfig?.supports_responses === true;
  const url = `${context.baseUrl}/${useResponses ? 'agent_v3' : 'agent_v2'}/chat/stream`;

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(context.accessToken ? { Authorization: `Bearer ${context.accessToken}` } : {}),
    },
    body: JSON.stringify({
      session_id: sessionId,
      content,
      user_id: userId,
      attachments: attachments || [],
      reasoning_enabled: reasoningEnabled,
      turn_id: options?.turnId,
      ai_model_config: {
        provider: modelConfig.provider,
        model_name: modelConfig.model_name,
        temperature: modelConfig.temperature || 0.7,
        thinking_level: modelConfig.thinking_level,
        api_key: modelConfig.api_key,
        base_url: modelConfig.base_url,
        supports_images: modelConfig.supports_images === true,
        supports_reasoning: modelConfig.supports_reasoning === true,
        supports_responses: modelConfig.supports_responses === true,
      },
    }),
  });
  context.applyRefreshedAccessToken(response);

  if (!response.ok) {
    throw new Error(`HTTP error! status: ${response.status}`);
  }

  if (!response.body) {
    throw new Error('Response body is null');
  }

  return response.body;
};

export const streamAgentChat = async (
  context: StreamApiContext,
  sessionId: string,
  content: string,
  agentId: string,
  userId?: string,
  attachments?: any[],
  reasoningEnabled?: boolean,
  options?: { useResponses?: boolean; turnId?: string }
): Promise<ReadableStream> => {
  const useResponses = options?.useResponses === true;
  const url = `${context.baseUrl}/${useResponses ? 'agent_v3/agents' : 'agents'}/chat/stream`;

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Accept': 'text/event-stream',
      ...(context.accessToken ? { Authorization: `Bearer ${context.accessToken}` } : {}),
    },
    body: JSON.stringify({
      session_id: sessionId,
      content,
      agent_id: agentId,
      user_id: userId,
      attachments: attachments || [],
      reasoning_enabled: reasoningEnabled,
      turn_id: options?.turnId,
    }),
  });
  context.applyRefreshedAccessToken(response);

  if (!response.ok) {
    throw new Error(`HTTP error! status: ${response.status}`);
  }

  if (!response.body) {
    throw new Error('Response body is null');
  }

  return response.body;
};

export const stopChat = (
  request: ApiRequestFn,
  sessionId: string,
  options?: { useResponses?: boolean }
): Promise<any> => {
  const useResponses = options?.useResponses === true;
  const path = useResponses ? '/agent_v3/chat/stop' : '/chat/stop';
  return request<any>(path, {
    method: 'POST',
    body: JSON.stringify({
      session_id: sessionId,
    }),
  });
};

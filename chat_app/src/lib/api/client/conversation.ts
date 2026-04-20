import { buildQuery } from './shared';
import type {
  AiModelConfigResponse,
  ConversationAssistantResponse,
  ConversationDetailsResponse,
  ConversationMessageEnvelope,
  ConversationMessagesEnvelope,
  ConversationMcpServersResponse,
  ConversationMessagePayload,
  McpConfigResourceResponse,
  McpConfigResponse,
  SessionResponse,
  SessionMessageResponse,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getConversationDetails = async (
  request: ApiRequestFn,
  conversationId: string,
): Promise<ConversationDetailsResponse> => {
  try {
    const session = await request<SessionResponse>(`/conversations/${conversationId}`);
    const createdAt = session.created_at || session.createdAt || new Date().toISOString();
    const updatedAt = session.updated_at || session.updatedAt || createdAt;
    return {
      data: {
        conversation: {
          id: session.id,
          title: session.title,
          created_at: createdAt,
          updated_at: updatedAt,
        },
      },
    };
  } catch (error) {
    console.error('Failed to get conversation details:', error);
    return {
      data: {
        conversation: {
          id: conversationId,
          title: 'Default Conversation',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      },
    };
  }
};

export const getAssistant = async (
  request: ApiRequestFn,
  _conversationId: string,
): Promise<ConversationAssistantResponse> => {
  try {
    const configs = await request<AiModelConfigResponse[]>('/ai-model-configs');
    const defaultConfig = configs.find((config) => config.enabled) || configs[0];

    if (!defaultConfig) {
      throw new Error('No AI model configuration found');
    }

    return {
      data: {
        assistant: {
          id: defaultConfig.id,
          name: defaultConfig.name,
          model_config: {
            model_name: defaultConfig.model_name,
            temperature: 0.7,
            api_key: defaultConfig.api_key,
            base_url: defaultConfig.base_url,
          },
        },
      },
    };
  } catch (error) {
    console.error('Failed to get assistant:', error);
    return {
      data: {
        assistant: {
          id: 'default-assistant',
          name: 'AI Assistant',
          model_config: {
            model_name: 'gpt-3.5-turbo',
            temperature: 0.7,
            api_key: '',
            base_url: 'https://api.openai.com/v1',
          },
        },
      },
    };
  }
};

export const getMcpServers = async (
  request: ApiRequestFn,
  _conversationId?: string,
): Promise<ConversationMcpServersResponse> => {
  try {
    const mcpConfigs = await request<McpConfigResponse[]>('/mcp-configs');
    const enabledServers = mcpConfigs
      .filter((config) => config.enabled)
      .map((config) => ({
        name: config.name,
        url: config.command,
      }));
    return {
      data: {
        mcp_servers: enabledServers,
      },
    };
  } catch (error) {
    console.error('Failed to get MCP servers:', error);
    return {
      data: {
        mcp_servers: [],
      },
    };
  }
};

export const getMcpConfigResource = async (
  request: ApiRequestFn,
  configId: string
): Promise<McpConfigResourceResponse> => {
  try {
    return await request<McpConfigResourceResponse>(`/mcp-configs/${configId}/resource/config`);
  } catch (error) {
    console.error('Failed to get MCP config resource:', error);
    return { success: false, config: null };
  }
};

export const getMcpConfigResourceByCommand = async (
  request: ApiRequestFn,
  data: {
    type: 'stdio' | 'http';
    command: string;
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    alias?: string | null;
  }
): Promise<McpConfigResourceResponse> => {
  try {
    return await request<McpConfigResourceResponse>(`/mcp-configs/resource/config`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  } catch (error) {
    console.error('Failed to get MCP config resource by command:', error);
    return { success: false, config: null };
  }
};

export const saveMessage = async (
  request: ApiRequestFn,
  conversationId: string,
  message: ConversationMessagePayload,
): Promise<ConversationMessageEnvelope> => {
  try {
    const messageId = message.id || `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

    const savedMessage = await request<SessionMessageResponse>(`/messages`, {
      method: 'POST',
      body: JSON.stringify({
        id: messageId,
        conversationId,
        role: message.role,
        content: message.content,
        toolCalls: message.tool_calls || null,
        toolCallId: message.tool_call_id || null,
        reasoning: message.reasoning || null,
        metadata: message.metadata || null,
      }),
    });

    return {
      data: {
        message: savedMessage,
      },
    };
  } catch (error) {
    console.error('Failed to save message:', error);
    const fallbackMessage: SessionMessageResponse = {
      id: message.id || Date.now().toString(),
      role: message.role,
      content: message.content,
      metadata: message.metadata || null,
      created_at: new Date().toISOString(),
    };

    if (Array.isArray(message.tool_calls)) {
      fallbackMessage.tool_calls = message.tool_calls;
    }

    return {
      data: {
        message: fallbackMessage,
      },
    };
  }
};

export const getMessages = async (
  request: ApiRequestFn,
  conversationId: string,
  params: { limit?: number; offset?: number } = {}
): Promise<ConversationMessagesEnvelope> => {
  try {
    const query = buildQuery({ limit: params.limit, offset: params.offset });
    const messages = await request<SessionMessageResponse[]>(
      `/conversations/${conversationId}/messages${query}`,
    );
    return {
      data: {
        messages,
      },
    };
  } catch (error) {
    console.error('Failed to get messages:', error);
    return {
      data: {
        messages: [],
      },
    };
  }
};

export const addMessage = (
  request: ApiRequestFn,
  conversationId: string,
  message: ConversationMessagePayload,
): Promise<ConversationMessageEnvelope> => {
  return saveMessage(request, conversationId, message);
};

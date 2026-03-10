import { buildQuery } from './shared';
import type { ApiRequestFn } from './workspace';

export const getConversationDetails = async (request: ApiRequestFn, conversationId: string) => {
  try {
    const session = await request<any>(`/sessions/${conversationId}`);
    return {
      data: {
        conversation: {
          id: session.id,
          title: session.title,
          created_at: session.created_at,
          updated_at: session.updated_at,
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

export const getAssistant = async (request: ApiRequestFn, _conversationId: string) => {
  try {
    const configs = await request<any[]>('/ai-model-configs');
    const defaultConfig = configs.find((config: any) => config.enabled) || configs[0];

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

export const getMcpServers = async (request: ApiRequestFn, _conversationId?: string) => {
  try {
    const mcpConfigs = await request<any[]>('/mcp-configs');
    const enabledServers = mcpConfigs
      .filter((config: any) => config.enabled)
      .map((config: any) => ({
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
): Promise<{ success: boolean; config: any; alias?: string }> => {
  try {
    return await request<any>(`/mcp-configs/${configId}/resource/config`);
  } catch (error) {
    console.error('Failed to get MCP config resource:', error);
    return { success: false, config: null } as any;
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
): Promise<{ success: boolean; config: any; alias?: string }> => {
  try {
    return await request<any>(`/mcp-configs/resource/config`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  } catch (error) {
    console.error('Failed to get MCP config resource by command:', error);
    return { success: false, config: null } as any;
  }
};

export const saveMessage = async (request: ApiRequestFn, conversationId: string, message: any) => {
  try {
    const messageId = message.id || `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

    const savedMessage = await request<any>(`/messages`, {
      method: 'POST',
      body: JSON.stringify({
        id: messageId,
        sessionId: conversationId,
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
    return {
      data: {
        message: {
          ...message,
          id: Date.now().toString(),
          created_at: new Date().toISOString(),
        },
      },
    };
  }
};

export const getMessages = async (
  request: ApiRequestFn,
  conversationId: string,
  params: { limit?: number; offset?: number } = {}
) => {
  try {
    const query = buildQuery({ limit: params.limit, offset: params.offset });
    const messages = await request<any[]>(`/sessions/${conversationId}/messages${query}`);
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

export const addMessage = (request: ApiRequestFn, conversationId: string, message: any) => {
  return saveMessage(request, conversationId, message);
};

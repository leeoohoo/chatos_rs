import { DatabaseService } from '../database';
import type { McpConfig } from '../database/schema';
import ApiClient from '../api/client';
// import McpToolExecute from './mcpToolExecute';
import { MessageManager } from './messageManager';
import { debugLog } from '@/lib/utils';

// 扩展DatabaseService以包含MCP相关方法
class ExtendedDatabaseService extends DatabaseService {
  constructor(userId: string, projectId: string) {
    super(userId, projectId);
  }

  async getAllMcpConfigs(): Promise<McpConfig[]> {
    // 实现获取所有MCP配置的逻辑
    return [];
  }

  async createMcpConfig(config: Omit<McpConfig, 'id' | 'createdAt' | 'updatedAt'>): Promise<McpConfig> {
    // 实现创建MCP配置的逻辑
    const newConfig: McpConfig = {
      ...config,
      id: Math.random().toString(36).substr(2, 9),
      createdAt: new Date(),
      updatedAt: new Date()
    };
    return newConfig;
  }



  async getUserConfig<T>(_key: string): Promise<T | null> {
    // 实现获取用户配置的逻辑
    return null;
  }
}

/**
 * 聊天配置接口
 */
export interface ChatConfig {
  model: string;
  temperature: number;
  apiKey: string;
  baseUrl: string;
}

/**
 * 聊天服务回调类型
 */
export interface ChatServiceCallbacks {
  onChunk?: (data: { type: string; content: string; accumulated?: string }) => void;
  onToolCall?: (toolCalls: any[]) => void;
  onToolResult?: (results: any[]) => void;
  onToolStreamChunk?: (data: { toolCallId?: string; tool_call_id?: string; chunk: string }) => void;
  onComplete?: (message: any) => void;
  onError?: (error: Error) => void;
}

/**
 * 聊天服务管理器
 */
export class ChatService {
  private currentAiClient: any = null;
  private currentSessionId: string | null = null; // 跟踪当前会话ID
  private currentModelConfig: any = null;
  private dbService: ExtendedDatabaseService;
  private messageManager: MessageManager;
  private userId: string;
  private configUrl: string;
  private apiClient: ApiClient;
  private currentStreamReader: ReadableStreamDefaultReader<Uint8Array> | null = null;

  constructor(userId: string, projectId: string, messageManager: MessageManager, configUrl?: string) {
    this.userId = userId;
    this.dbService = new ExtendedDatabaseService(userId, projectId);
    this.messageManager = messageManager;
    this.configUrl = configUrl || '/api'; // 使用相对路径作为默认值
    this.apiClient = new ApiClient(this.configUrl);
    debugLog('🔧 ChatService Constructor - configUrl:', this.configUrl);
  }



  /**
   * 发送消息并处理AI响应
   */
  async sendMessage(
    sessionId: string,
    content: string,
    _attachments: any[] = [],
    callbacks: ChatServiceCallbacks = {},
    modelConfig?: {
      model_name: string;
      temperature: number;
      api_key: string;
      base_url: string;
      provider?: string;
      thinking_level?: string;
      supports_reasoning?: boolean;
      supports_responses?: boolean;
    }
  ): Promise<void> {
    try {
      // 设置当前会话ID
      this.currentSessionId = sessionId;
      // 维持 MessageManager 引用（避免未使用警告）
      void this.messageManager;
      
      // 获取会话信息
      const session = await this.dbService.getSession(sessionId);
      if (!session) {
        throw new Error('Session not found');
      }


      let finalModelConfig;
      if (modelConfig) {
        finalModelConfig = modelConfig;
      } else {
        const chatConfig = await this.getChatConfig();
        finalModelConfig = {
          model_name: chatConfig.model,
          temperature: chatConfig.temperature,
          api_key: chatConfig.apiKey,
          base_url: chatConfig.baseUrl,
          provider: 'gpt'
        };
      }
      this.currentModelConfig = finalModelConfig;


      // 通过后端流式接口发送消息（MCP 由后端按标准协议处理）
      const safeAttachments = Array.isArray(_attachments)
        ? _attachments.map((att) => {
            if (typeof File !== 'undefined' && att instanceof File) {
              return {
                name: att.name,
                mimeType: att.type,
                size: att.size,
                type: att.type?.startsWith('image/') ? 'image' : 'file'
              };
            }
            return att;
          }).filter(Boolean)
        : [];

      const stream = await this.apiClient.streamChat(
        sessionId,
        content,
        finalModelConfig,
        this.userId,
        safeAttachments
      );

      const reader = stream.getReader();
      this.currentStreamReader = reader;

      const decoder = new TextDecoder();
      let buffer = '';
      let completed = false;

      const handleComplete = (data: any = null) => {
        if (completed) return;
        completed = true;
        callbacks.onComplete?.(data);
      };

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed.startsWith(':')) continue;
          if (!trimmed.startsWith('data:')) continue;

          const dataStr = trimmed.slice(5).trim();
          if (!dataStr) continue;
          if (dataStr === '[DONE]') {
            handleComplete();
            break;
          }

          let parsed: any = null;
          try {
            parsed = JSON.parse(dataStr);
          } catch (e) {
            continue;
          }

          if (parsed && typeof parsed === 'string' && parsed === '[DONE]') {
            handleComplete();
            break;
          }

          const type = parsed?.type;
          try {
            switch (type) {
              case 'chunk': {
                const contentChunk = typeof parsed.content === 'string' ? parsed.content : '';
                if (contentChunk) {
                  callbacks.onChunk?.({
                    type: 'text',
                    content: contentChunk,
                    accumulated: parsed.accumulated || contentChunk
                  });
                }
                break;
              }
              case 'tools_start': {
                const toolCalls = parsed?.data?.tool_calls || parsed?.data || [];
                const toolCallsArray = Array.isArray(toolCalls) ? toolCalls : [toolCalls];
                callbacks.onToolCall?.(toolCallsArray);
                break;
              }
              case 'tools_stream': {
                const data = parsed?.data || {};
                const toolCallId = data.tool_call_id || data.toolCallId || data.id;
                const chunk = data.content || data.chunk || data.data || '';
                callbacks.onToolStreamChunk?.({
                  toolCallId,
                  tool_call_id: toolCallId,
                  chunk
                } as any);
                break;
              }
              case 'tools_end': {
                const resultsRaw = parsed?.data?.tool_results || parsed?.data || [];
                const resultsArray = Array.isArray(resultsRaw) ? resultsRaw : [resultsRaw];
                const normalized = resultsArray.map((r: any) => ({
                  ...r,
                  tool_call_id: r.tool_call_id || r.id || r.toolCallId,
                  result: r.result ?? r.content
                }));
                callbacks.onToolResult?.(normalized);
                break;
              }
              case 'complete': {
                handleComplete(parsed?.result ?? parsed?.data ?? null);
                break;
              }
              case 'error': {
                const message = parsed?.message || parsed?.data?.error || parsed?.data?.message || 'Stream error';
                callbacks.onError?.(new Error(message));
                break;
              }
              case 'cancelled': {
                handleComplete();
                break;
              }
              default:
                break;
            }
          } catch (callbackError) {
            console.error('Callback error:', callbackError);
            callbacks.onError?.(new Error(`处理AI响应时出错: ${callbackError instanceof Error ? callbackError.message : '未知错误'}`));
          }
        }
      }

      handleComplete();

    } catch (error: any) {
      // 检查是否是用户中断错误
      if (error.message === 'Stream aborted by user' || error.name === 'AbortError') {
        debugLog('Message sending aborted by user');
        return;
      }
      
      // 检查是否是网络连接错误
      if (error.message?.includes('ERR_INCOMPLETE_CHUNKED_ENCODING') || 
          error.message?.includes('net::ERR_') ||
          error.message?.includes('Failed to fetch')) {
        debugLog('Network connection error during streaming:', error.message);
        callbacks.onError?.(new Error('网络连接中断，请检查网络状态后重试'));
        return;
      }
      
      console.error('Failed to send message:', error);
      callbacks.onError?.(error instanceof Error ? error : new Error(String(error)));
      throw error;
    } finally {
      if (this.currentStreamReader) {
        try {
          this.currentStreamReader.releaseLock();
        } catch (_) {}
        this.currentStreamReader = null;
      }
    }
  }

  /**
   * 中止当前对话
   */
  async abortCurrentConversation(): Promise<void> {
    debugLog('🛑 ChatService: 中止当前对话');
    
    if (this.currentSessionId) {
      try {
        debugLog(`🛑 ChatService: 调用服务端停止接口，会话ID: ${this.currentSessionId}`);
        
        // 调用服务端停止接口（支持 IPC）
        await this.apiClient.stopChat(this.currentSessionId, { useResponses: this.currentModelConfig?.supports_responses === true });

        debugLog('✅ ChatService: 服务端停止成功');

        // 取消本地读取
        if (this.currentStreamReader) {
          try { await this.currentStreamReader.cancel(); } catch (_) {}
          this.currentStreamReader = null;
        }

        // 清理本地状态
            // 如果服务端停止失败，尝试客户端停止作为备用方案
        if (this.currentAiClient) {
          debugLog('🔄 ChatService: 尝试客户端停止作为备用方案');
          this.currentAiClient.abort();
        }
        this.currentAiClient = null;
        this.currentSessionId = null;
        
      } catch (error) {
        console.error('❌ ChatService: 调用服务端停止接口失败:', error);
        
        // 如果服务端停止失败，尝试客户端停止作为备用方案
        if (this.currentAiClient) {
          debugLog('🔄 ChatService: 尝试客户端停止作为备用方案');
          this.currentAiClient.abort();
        }

        if (this.currentStreamReader) {
          try { await this.currentStreamReader.cancel(); } catch (_) {}
          this.currentStreamReader = null;
        }
        
        // 清理本地状态
        this.currentAiClient = null;
        this.currentSessionId = null;
      }
    } else {
      debugLog('⚠️ ChatService: 没有活动的会话可以中止');
      
      // 如果没有会话ID但有AI客户端，仍然尝试停止
      if (this.currentAiClient) {
        debugLog('🔄 ChatService: 尝试停止当前AI客户端');
        this.currentAiClient.abort();
        this.currentAiClient = null;
      }
    }
  }







  /**
   * 获取聊天配置
   */
  async getChatConfig(): Promise<ChatConfig> {
    const config = await this.dbService.getUserConfig<ChatConfig>('chatConfig');
    return config || {
      model: 'gpt-3.5-turbo',
      temperature: 0.7,
      apiKey: '',
      baseUrl: 'https://api.openai.com/v1'
    };
  }



}

// 导出核心服务类
export { default as AiServer } from './aiServer';
export { MessageManager } from './messageManager';

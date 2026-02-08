import { apiClient } from '../api/client';
import type ApiClient from '../api/client';
import type { Session, Message } from './mock';
import { debugLog } from '@/lib/utils';

// 数据库初始化
export function initDatabase(): Promise<void> {
  debugLog('API client initialized');
  return Promise.resolve();
}



// 关闭数据库连接
export function closeDatabase(): Promise<void> {
  debugLog('Mock database closed');
  return Promise.resolve();
}

// 数据库服务类
export class DatabaseService {
  private userId: string;
  private projectId: string;
  private client: ApiClient;

  constructor(userId: string, projectId: string, client: ApiClient = apiClient) {
    this.userId = userId;
    this.projectId = projectId;
    this.client = client;
  }

  // 会话相关操作
  async createSession(data: Omit<Session, 'id'>): Promise<Session> {
    const sessionData = { id: crypto.randomUUID(), title: data.title, user_id: this.userId, project_id: this.projectId };
    const session = await this.client.createSession(sessionData);
    return {
      id: session.id,
      title: session.title,
      createdAt: new Date(session.created_at),
      updatedAt: new Date(session.updated_at),
      messageCount: 0,
      tokenUsage: 0,
      pinned: false,
      archived: false,
      tags: null,
      metadata: null
    };
  }

  async getSession(id: string): Promise<Session | null> {
    try {
      const session = await this.client.getSession(id);
      if (!session) return null;
      return {
        id: session.id,
        title: session.title,
        createdAt: new Date(session.created_at),
        updatedAt: new Date(session.updated_at),
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        tags: null,
        metadata: null
      };
    } catch (error) {
      return null;
    }
  }

  async getAllSessions(): Promise<Session[]> {
    debugLog('🔍 DatabaseService.getAllSessions 调用:', { userId: this.userId, projectId: this.projectId });
    
    const sessions = await this.client.getSessions(this.userId, this.projectId);
    debugLog('🔍 API返回的会话数据:', sessions);
    
    // 转换字段名：数据库使用下划线命名，前端使用驼峰命名
    const formattedSessions = sessions.map((session: any) => ({
      id: session.id,
      title: session.title,
      createdAt: new Date(session.created_at || session.createdAt),
      updatedAt: new Date(session.updated_at || session.updatedAt),
      messageCount: 0, // 默认值，可以后续优化
      tokenUsage: 0, // 默认值，可以后续优化
      pinned: false, // 默认值
      archived: false, // 默认值
      tags: null,
      metadata: null
    }));
    
    debugLog('🔍 格式化后的会话数据:', formattedSessions);
    return formattedSessions;
  }

  async updateSession(_id: string, _updates: Partial<Session>): Promise<Session | null> {
    // TODO: 实现更新会话API
    console.warn('updateSession not implemented yet');
    return null;
  }

  async deleteSession(id: string): Promise<boolean> {
    try {
      await this.client.deleteSession(id);
      return true;
    } catch (error) {
      return false;
    }
  }

  // 消息相关操作
  async createMessage(data: Omit<Message, 'id'>): Promise<Message> {
    const messageData = {
      id: crypto.randomUUID(),
      session_id: data.sessionId,
      role: data.role,
      content: data.content,
      summary: data.rawContent,
      tool_calls: data.metadata?.toolCalls ? JSON.stringify(data.metadata.toolCalls) : undefined,
      tool_call_id: data.toolCallId,
      reasoning: undefined,
      metadata: data.metadata ? JSON.stringify(data.metadata) : undefined
    };
    const messageRequestData = {
      id: messageData.id,
      sessionId: messageData.session_id,
      role: messageData.role,
      content: messageData.content,
      metadata: messageData.metadata ? (typeof messageData.metadata === 'string' ? JSON.parse(messageData.metadata) : messageData.metadata) : undefined,
      toolCalls: messageData.tool_calls ? (typeof messageData.tool_calls === 'string' ? JSON.parse(messageData.tool_calls) : messageData.tool_calls) : undefined
    };
    const result = await this.client.createMessage(messageRequestData);
    return {
      id: result.id,
      sessionId: result.session_id,
      role: result.role as any,
      content: result.content,
      rawContent: result.summary,
      tokensUsed: data.tokensUsed,
      status: data.status || 'completed',
      createdAt: new Date(result.created_at),
      updatedAt: data.updatedAt,
      toolCallId: result.tool_call_id,
      metadata: result.metadata ? (typeof result.metadata === 'string' ? JSON.parse(result.metadata) : result.metadata) : data.metadata
    };
  }

  async getSessionMessages(sessionId: string, options: { limit?: number; offset?: number } = { limit: 10, offset: 0 }): Promise<Message[]> {
    // 默认只加载最近10条，支持传入 offset 实现“加载更多”
    const limit = options.limit ?? 10;
    const offset = options.offset ?? 0;
    const messages = await this.client.getSessionMessages(sessionId, { limit, offset });
    
    // 第一步：解析所有消息并收集工具调用和结果
    const parsedMessages = messages.map(message => {
      // 解析metadata
      let parsedMetadata = undefined;
      if (message.metadata) {
        try {
          parsedMetadata = typeof message.metadata === 'string' ? JSON.parse(message.metadata) : message.metadata;
        } catch (error) {
          console.warn('Failed to parse message metadata:', error);
          parsedMetadata = {};
        }
      }

      // 解析顶层的toolCalls（兼容后端可能同时在顶层和metadata中存储）
      let parsedTopLevelToolCalls = undefined;
      if (message.toolCalls) {
        try {
          parsedTopLevelToolCalls = typeof message.toolCalls === 'string' ? JSON.parse(message.toolCalls) : message.toolCalls;
        } catch (error) {
          console.warn('Failed to parse top-level toolCalls:', error);
        }
      }

      return {
        id: message.id,
        sessionId: message.session_id,
        role: message.role as 'user' | 'assistant' | 'system' | 'tool',
        content: message.content,
        summary: message.summary,
        toolCallId: message.tool_call_id,
        reasoning: message.reasoning,
        metadata: parsedMetadata,
        topLevelToolCalls: parsedTopLevelToolCalls, // 保存顶层的toolCalls
        createdAt: new Date(message.created_at),
        originalMessage: message
      };
    });

    // 第二步：建立工具调用ID到结果的映射
    const toolResultsMap = new Map<string, { content: string; error?: string }>();
    
    parsedMessages.forEach(msg => {
      if (msg.role === 'tool' && msg.toolCallId) {
        // 工具结果消息
        const isError = msg.metadata?.isError || false;
        toolResultsMap.set(msg.toolCallId, {
          content: msg.content,
          error: isError ? msg.content : undefined
        });
      }
    });

    // 第三步：处理工具调用并关联结果，构建标准消息
    const normalized = parsedMessages.map(msg => {
      let toolCalls = undefined;

      // 优先使用顶层的toolCalls，如果没有再使用metadata中的
      const sourceToolCalls = (msg as any).topLevelToolCalls || msg.metadata?.toolCalls;

      if (msg.role === 'assistant' && sourceToolCalls && Array.isArray(sourceToolCalls)) {
        debugLog('[Database] 处理工具调用:', { messageId: msg.id, sourceToolCalls });
        toolCalls = sourceToolCalls.map((toolCall: any) => {
          if (toolCall.function) {
            // 解析工具调用参数
            let parsedArguments = {};
            try {
              parsedArguments = typeof toolCall.function.arguments === 'string' 
                ? JSON.parse(toolCall.function.arguments) 
                : toolCall.function.arguments;
            } catch (error) {
              console.warn('Failed to parse tool call arguments:', error);
              parsedArguments = {};
            }

            // 查找对应的工具结果
            const toolResult = toolResultsMap.get(toolCall.id);

            return {
              id: toolCall.id || `tool_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
              messageId: msg.id,
              name: toolCall.function.name,
              arguments: parsedArguments,
              result: toolResult?.content || undefined,
              error: toolResult?.error || undefined,
              createdAt: msg.createdAt
            };
          }
          return null;
        }).filter(Boolean);
      }

      // 构建内容分段：优先将 reasoning 作为独立的 thinking 段，其次正文为 text 段，最后添加工具调用段
      const contentSegments: any[] = [];
      const hasReasoning = typeof msg.reasoning === 'string' && msg.reasoning.trim().length > 0;
      if (msg.role === 'assistant' && hasReasoning) {
        contentSegments.push({ type: 'thinking', content: msg.reasoning });
      }
      if (typeof msg.content === 'string' && msg.content.trim().length > 0) {
        contentSegments.push({ type: 'text', content: msg.content });
      }
      // 为每个工具调用添加 tool_call 段
      if (toolCalls && Array.isArray(toolCalls) && toolCalls.length > 0) {
        toolCalls.forEach((tc: any) => {
          contentSegments.push({ type: 'tool_call', toolCallId: tc.id });
        });
      }

       // 归一化附件：后端为节省存储仅保留 preview（小图 base64），这里将其映射为前端可用的 url
       let normalizedAttachments: any[] | undefined = undefined;
       try {
         const rawAtts: any[] = (msg.metadata && (msg.metadata as any).attachments) || [];
         if (Array.isArray(rawAtts) && rawAtts.length > 0) {
           normalizedAttachments = rawAtts.map((a: any, idx: number) => {
             const mime = a.mimeType || a.mime || 'application/octet-stream';
             const hasPreview = Boolean(a.preview || a.url);
             const baseType = mime.startsWith('image/') ? 'image' : (mime.startsWith('audio/') ? 'audio' : 'file');
             const type = hasPreview ? (a.type || baseType) : (baseType === 'image' ? 'file' : baseType);
             return {
               id: a.id || `${msg.id}_att_${idx}`,
               messageId: msg.id,
               type,
               name: a.name || `attachment-${idx + 1}`,
               url: a.preview || a.url || '', // 关键：映射 preview -> url 以供前端显示
               size: a.size || 0,
               mimeType: mime,
               createdAt: msg.createdAt
             };
           });
         }
       } catch (_) {}

       return {
         id: msg.id,
         sessionId: msg.sessionId,
         role: msg.role,
         content: msg.content,
         rawContent: msg.summary,
         tokensUsed: undefined,
         status: 'completed' as const,
         createdAt: msg.createdAt,
         updatedAt: undefined,
         toolCallId: msg.toolCallId,
         metadata: {
           ...msg.metadata,
           ...(normalizedAttachments ? { attachments: normalizedAttachments } : {}),
           toolCalls: toolCalls,
           contentSegments: contentSegments.length > 0 ? contentSegments : msg.metadata?.contentSegments
         }
       };
    });

    // 第四步：吸收会话摘要消息（metadata.type === 'session_summary'）
    // 将其转换为“当前或下一条助手消息”的一个 thinking 分段，保证历史渲染与流式一致
    // 标记需要隐藏的“摘要记录”，避免前端重复渲染为独立卡片，但保留条目用于分页计数
    const toHide = new Set<string>();
    for (let i = 0; i < normalized.length; i++) {
      const m = normalized[i];
      if (m?.metadata?.type === 'session_summary') {
        // 取摘要文本：优先 rawContent(summary)，其次 metadata.summary/content
        const summaryText = (typeof m.rawContent === 'string' && m.rawContent.length > 0)
          ? m.rawContent
          : (typeof (m.metadata as any)?.summary === 'string' && (m.metadata as any).summary.length > 0)
            ? (m.metadata as any).summary
            : (typeof m.content === 'string' ? m.content : '');

        // 找到“下一条助手消息”（若没有则回退上一条助手消息）
        let targetIdx = -1;
        for (let j = i + 1; j < normalized.length; j++) {
          if (normalized[j]?.role === 'assistant') { targetIdx = j; break; }
        }
        if (targetIdx === -1) {
          for (let j = i - 1; j >= 0; j--) {
            if (normalized[j]?.role === 'assistant') { targetIdx = j; break; }
          }
        }

        if (targetIdx !== -1) {
          const target = normalized[targetIdx];
          const header = '【上下文摘要】\n';
          const segs = (target.metadata?.contentSegments || []).slice();
          // 避免重复注入：若最后一个 thinking 段以 header 开头则拼接，否则新增
          const lastIdx = segs.length - 1;
          if (lastIdx >= 0 && segs[lastIdx].type === 'thinking' && String((segs[lastIdx] as any).content || '').startsWith(header)) {
            (segs[lastIdx] as any).content = header + String(summaryText || '');
          } else {
            segs.push({ type: 'thinking', content: header + String(summaryText || '') });
          }
          target.metadata = target.metadata || {} as any;
          (target.metadata as any).contentSegments = segs;
          // 标记该摘要消息在前端隐藏（避免重复渲染卡片，同时不影响分页长度判断）
          m.metadata = (m.metadata || {}) as any;
          (m.metadata as any).hidden = true;
          toHide.add(m.id);
        }
      }
    }
    return normalized;
  }

  async updateMessage(_id: string, _updates: Partial<Message>): Promise<Message | null> {
    // TODO: 实现更新消息API
    console.warn('updateMessage not implemented yet');
    return null;
  }

  async deleteMessage(_id: string): Promise<boolean> {
    // TODO: 实现删除消息API
    console.warn('deleteMessage not implemented yet');
    return false;
  }

  // 私有方法：更新会话统计
  // private async updateSessionStats(_sessionId: string): Promise<void> {
  //   const messages = await this.getSessionMessages(_sessionId);
  //   const messageCount = messages.length;
  //   
  //   await this.updateSession(_sessionId, {
  //     messageCount,
  //     updatedAt: new Date()
  //   });
  // }
}

// 导出类型
export type { Session, Message };

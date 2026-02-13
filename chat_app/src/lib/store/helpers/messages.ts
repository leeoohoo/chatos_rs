import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';


const parseMaybeJson = (value: any): any => {
  if (typeof value !== 'string') return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
};

const normalizeToolCallsArray = (value: any): any[] => {
  const parsed = parseMaybeJson(value);
  if (Array.isArray(parsed)) return parsed;
  if (parsed && typeof parsed === 'object') return [parsed];
  return [];
};

const extractToolCallsFromMessage = (message: any): any[] => {
  const topLevel = normalizeToolCallsArray(message?.toolCalls);
  if (topLevel.length > 0) return topLevel;

  const parsedMetadata = parseMaybeJson(message?.metadata);
  return normalizeToolCallsArray(parsedMetadata?.toolCalls);
};

const collectMissingAssistantToolCallIds = (messages: any[]): string[] => {
  const assistantCallIds = new Set<string>();
  const toolResultIds = new Set<string>();

  messages.forEach((message: any) => {
    if (message?.role === 'assistant') {
      extractToolCallsFromMessage(message).forEach((toolCall: any) => {
        const id = toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId;
        if (id) assistantCallIds.add(String(id));
      });
    }

    if (message?.role === 'tool') {
      const id = message?.tool_call_id || message?.toolCallId;
      if (id) toolResultIds.add(String(id));
    }
  });

  return Array.from(toolResultIds).filter((id) => !assistantCallIds.has(id));
};

export const fetchSessionMessages = async (
  client: ApiClient,
  sessionId: string,
  options: { limit?: number; offset?: number } = { limit: 50, offset: 0 }
): Promise<Message[]> => {
  const limit = options.limit ?? 50;
  const offset = options.offset ?? 0;

  let rawMessages = await client.getSessionMessages(sessionId, { limit, offset });

  // When a long tool run inserts many rows, the latest page can contain tool results but miss
  // the corresponding assistant tool_call row, which breaks modal reconstruction after session switch.
  if (offset === 0 && limit > 0) {
    const seenIds = new Set<string>(rawMessages.map((message: any) => String(message?.id || '')));
    let nextOffset = offset + rawMessages.length;

    for (let attempt = 0; attempt < 4; attempt += 1) {
      const missingToolCallIds = collectMissingAssistantToolCallIds(rawMessages);
      if (missingToolCallIds.length === 0) break;

      const older = await client.getSessionMessages(sessionId, { limit, offset: nextOffset });
      if (!Array.isArray(older) || older.length === 0) break;
      nextOffset += older.length;

      const dedupOlder = older.filter((message: any) => {
        const id = String(message?.id || '');
        if (!id || seenIds.has(id)) return false;
        seenIds.add(id);
        return true;
      });
      if (dedupOlder.length === 0) break;

      rawMessages = [...dedupOlder, ...rawMessages];
    }
  }

  const parsedMessages = rawMessages.map((message: any) => {
    let parsedMetadata = undefined;
    if (message.metadata) {
      try {
        parsedMetadata = typeof message.metadata === 'string' ? JSON.parse(message.metadata) : message.metadata;
      } catch (error) {
        console.warn('Failed to parse message metadata:', error);
        parsedMetadata = {};
      }
    }

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
      topLevelToolCalls: parsedTopLevelToolCalls,
      createdAt: new Date(message.created_at),
      originalMessage: message
    };
  });

  const toolResultsMap = new Map<string, { content: string; error?: string }>();

  parsedMessages.forEach(msg => {
    if (msg.role === 'tool' && msg.toolCallId) {
      const isError = msg.metadata?.isError || false;
      toolResultsMap.set(msg.toolCallId, {
        content: msg.content,
        error: isError ? msg.content : undefined
      });
    }
  });

  const normalized = parsedMessages.map(msg => {
    let toolCalls = undefined;

    const sourceToolCalls = (msg as any).topLevelToolCalls || msg.metadata?.toolCalls;

    if (msg.role === 'assistant' && sourceToolCalls && Array.isArray(sourceToolCalls)) {
      debugLog('[Store] 处理工具调用:', { messageId: msg.id, sourceToolCalls });
      toolCalls = sourceToolCalls.map((toolCall: any) => {
        const toolCallId = toolCall.id || toolCall.tool_call_id || toolCall.toolCallId;
        const toolResult = toolCallId ? toolResultsMap.get(String(toolCallId)) : undefined;

        if (toolCall.function) {
          let parsedArguments = {};
          try {
            parsedArguments = typeof toolCall.function.arguments === 'string'
              ? JSON.parse(toolCall.function.arguments)
              : toolCall.function.arguments;
          } catch (error) {
            console.warn('Failed to parse tool call arguments:', error);
            parsedArguments = {};
          }

          return {
            id: toolCallId || `tool_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
            messageId: msg.id,
            name: toolCall.function.name,
            arguments: parsedArguments,
            result: toolResult?.content || undefined,
            error: toolResult?.error || undefined,
            createdAt: msg.createdAt
          };
        }

        let parsedArguments = toolCall.arguments ?? toolCall.args ?? {};
        if (typeof parsedArguments === 'string') {
          try {
            parsedArguments = JSON.parse(parsedArguments);
          } catch {
            // keep raw string when it's not JSON
          }
        }

        return {
          id: toolCallId || `tool_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
          messageId: msg.id,
          name: toolCall.name || toolCall.tool_name || toolCall.toolName || 'unknown_tool',
          arguments: parsedArguments,
          result: toolCall.result ?? toolCall.finalResult ?? toolCall.final_result ?? toolResult?.content,
          finalResult: toolCall.finalResult ?? toolCall.final_result,
          streamLog: toolCall.streamLog ?? toolCall.stream_log ?? '',
          completed: toolCall.completed === true,
          error: toolCall.error || toolResult?.error || undefined,
          createdAt: toolCall.createdAt || toolCall.created_at || msg.createdAt,
        };
      }).filter(Boolean);
    }

    const contentSegments: any[] = [];
    const hasReasoning = typeof msg.reasoning === 'string' && msg.reasoning.trim().length > 0;
    if (msg.role === 'assistant' && hasReasoning) {
      contentSegments.push({ type: 'thinking', content: msg.reasoning });
    }
    if (typeof msg.content === 'string' && msg.content.trim().length > 0) {
      contentSegments.push({ type: 'text', content: msg.content });
    }
    if (toolCalls && Array.isArray(toolCalls) && toolCalls.length > 0) {
      toolCalls.forEach((tc: any) => {
        contentSegments.push({ type: 'tool_call', toolCallId: tc.id });
      });
    }

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
            url: a.preview || a.url || '',
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

  const toHide = new Set<string>();
  for (let i = 0; i < normalized.length; i++) {
    const m = normalized[i];
    if (m?.metadata?.type === 'session_summary') {
      const summaryText = (typeof m.rawContent === 'string' && m.rawContent.length > 0)
        ? m.rawContent
        : (typeof (m.metadata as any)?.summary === 'string' && (m.metadata as any).summary.length > 0)
          ? (m.metadata as any).summary
          : (typeof m.content === 'string' ? m.content : '');

      let targetIdx = -1;
      for (let j = i + 1; j < normalized.length; j++) {
        if (normalized[j]?.role === 'assistant') { targetIdx = j; break; }
      }
      if (targetIdx == -1) {
        for (let j = i - 1; j >= 0; j--) {
          if (normalized[j]?.role === 'assistant') { targetIdx = j; break; }
        }
      }

      if (targetIdx !== -1) {
        const target = normalized[targetIdx];
        const header = '【上下文摘要】\n';
        const segs = (target.metadata?.contentSegments || []).slice();
        const lastIdx = segs.length - 1;
        if (lastIdx >= 0 && segs[lastIdx].type === 'thinking' && String((segs[lastIdx] as any).content || '').startsWith(header)) {
          (segs[lastIdx] as any).content = header + String(summaryText || '');
        } else {
          segs.push({ type: 'thinking', content: header + String(summaryText || '') });
        }
        target.metadata = target.metadata || {} as any;
        (target.metadata as any).contentSegments = segs;
        m.metadata = (m.metadata || {}) as any;
        (m.metadata as any).hidden = true;
        toHide.add(m.id);
      }
    }
  }

  return normalized;
};

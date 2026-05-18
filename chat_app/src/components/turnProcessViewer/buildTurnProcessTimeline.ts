import type {
  ContentSegment,
  Message,
  ToolCall,
  UnavailableToolInfo,
} from '../../types';

export type TurnProcessTimelineItem =
  | {
    id: string;
    kind: 'thinking';
    createdAt: Date | null;
    text: string;
    isStreaming: boolean;
  }
  | {
    id: string;
    kind: 'tool_call';
    createdAt: Date | null;
    toolCall: ToolCall;
    streamLog: string;
    completed: boolean;
  }
  | {
    id: string;
    kind: 'tool_unavailable';
    createdAt: Date | null;
    entry: UnavailableToolInfo;
  };

const toDate = (value: unknown): Date | null => {
  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? null : value;
  }
  if (typeof value === 'string' || typeof value === 'number') {
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? null : parsed;
  }
  return null;
};

const normalizeSegmentText = (segment: ContentSegment | null | undefined): string => {
  if (!segment) {
    return '';
  }
  if (typeof segment.content === 'string') {
    return segment.content.trim();
  }
  return '';
};

const normalizeStreamLog = (toolCall: ToolCall & { streamLog?: string }): string => (
  typeof toolCall.streamLog === 'string' ? toolCall.streamLog.trim() : ''
);

export const buildTurnProcessTimeline = ({
  processMessages,
  fallbackAssistantMessage,
}: {
  processMessages: Message[];
  fallbackAssistantMessage: Message | null;
}): TurnProcessTimelineItem[] => {
  const items: TurnProcessTimelineItem[] = [];
  const seenToolCallIds = new Set<string>();
  const seenUnavailableIds = new Set<string>();
  const seenThinkingKeys = new Set<string>();

  const assistantMessages = processMessages.filter((message) => message.role === 'assistant');

  if (
    fallbackAssistantMessage
    && fallbackAssistantMessage.role === 'assistant'
    && !assistantMessages.some((message) => message.id === fallbackAssistantMessage.id)
  ) {
    assistantMessages.push(fallbackAssistantMessage);
  }

  assistantMessages.forEach((message, messageIndex) => {
    const createdAt = toDate(message.updatedAt || message.createdAt);
    const contentSegments = Array.isArray(message.metadata?.contentSegments)
      ? message.metadata.contentSegments as ContentSegment[]
      : [];
    const toolCalls = Array.isArray(message.metadata?.toolCalls)
      ? message.metadata.toolCalls as Array<ToolCall & { streamLog?: string; completed?: boolean }>
      : [];
    const unavailableTools = Array.isArray(message.metadata?.unavailableTools)
      ? message.metadata.unavailableTools as UnavailableToolInfo[]
      : [];
    const toolCallsById = new Map<string, ToolCall & { streamLog?: string; completed?: boolean }>();

    toolCalls.forEach((toolCall) => {
      if (toolCall?.id) {
        toolCallsById.set(toolCall.id, toolCall);
      }
    });

    contentSegments.forEach((segment, segmentIndex) => {
      if (segment?.type === 'thinking') {
        const text = normalizeSegmentText(segment);
        if (!text) {
          return;
        }
        const dedupeKey = `${message.id}:thinking:${text}`;
        if (seenThinkingKeys.has(dedupeKey)) {
          return;
        }
        seenThinkingKeys.add(dedupeKey);
        items.push({
          id: `${message.id}:thinking:${segmentIndex}`,
          kind: 'thinking',
          createdAt,
          text,
          isStreaming: message.status === 'streaming',
        });
        return;
      }

      if (segment?.type === 'tool_call' && typeof segment.toolCallId === 'string' && segment.toolCallId.trim()) {
        const toolCallId = segment.toolCallId.trim();
        if (seenToolCallIds.has(toolCallId)) {
          return;
        }
        seenToolCallIds.add(toolCallId);
        const toolCall = toolCallsById.get(toolCallId) || {
          id: toolCallId,
          messageId: message.id,
          name: 'unknown_tool',
          arguments: {},
          createdAt: createdAt || new Date(),
        };
        items.push({
          id: `${message.id}:tool:${toolCallId}`,
          kind: 'tool_call',
          createdAt: toDate(toolCall.createdAt) || createdAt,
          toolCall,
          streamLog: normalizeStreamLog(toolCall),
          completed: (toolCall as { completed?: boolean }).completed === true,
        });
      }
    });

    toolCalls.forEach((toolCall, toolCallIndex) => {
      if (!toolCall?.id || seenToolCallIds.has(toolCall.id)) {
        return;
      }
      seenToolCallIds.add(toolCall.id);
      items.push({
        id: `${message.id}:tool-fallback:${toolCall.id}:${toolCallIndex}`,
        kind: 'tool_call',
        createdAt: toDate(toolCall.createdAt) || createdAt,
        toolCall,
        streamLog: normalizeStreamLog(toolCall),
        completed: toolCall.completed === true,
      });
    });

    unavailableTools.forEach((entry, unavailableIndex) => {
      const dedupeId = entry?.id || `${message.id}:unavailable:${unavailableIndex}`;
      if (seenUnavailableIds.has(dedupeId)) {
        return;
      }
      seenUnavailableIds.add(dedupeId);
      items.push({
        id: `${message.id}:unavailable:${dedupeId}`,
        kind: 'tool_unavailable',
        createdAt: toDate(entry.createdAt) || createdAt,
        entry,
      });
    });

    void messageIndex;
  });

  return items.sort((left, right) => {
    const leftTs = left.createdAt?.getTime() || 0;
    const rightTs = right.createdAt?.getTime() || 0;
    if (leftTs !== rightTs) {
      return leftTs - rightTs;
    }
    return left.id.localeCompare(right.id);
  });
};

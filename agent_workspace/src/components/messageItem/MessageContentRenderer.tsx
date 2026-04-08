import React from 'react';
import { MarkdownRenderer } from '../MarkdownRenderer';
import type { Message, ToolCall } from '../../types';
import type { RenderSegment, ToolCallLookupMap } from './types';
import { ToolCallTimeline } from './ToolCallTimeline';
import { normalizeMetaId } from './helpers';

interface MessageContentRendererProps {
  message: Message;
  isLast: boolean;
  isStreaming: boolean;
  renderContentSegments: RenderSegment[];
  toolCalls: ToolCall[];
  toolCallsById: ToolCallLookupMap;
  assistantToolCallsById?: ToolCallLookupMap;
  toolResultById?: Map<string, Message>;
  collapseAssistantProcessByDefault: boolean;
  hideInternalProcess?: boolean;
  onApplyCode: (code: string, language: string) => void;
}

export const MessageContentRenderer: React.FC<MessageContentRendererProps> = ({
  message,
  isLast,
  isStreaming,
  renderContentSegments,
  toolCalls,
  toolCallsById,
  assistantToolCallsById,
  toolResultById,
  collapseAssistantProcessByDefault,
  hideInternalProcess = false,
  onApplyCode,
}) => {
  const hasContent = message.content && message.content.trim().length > 0;
  const isCurrentlyStreaming = isStreaming && isLast;

  if (renderContentSegments.length > 0) {
    const nodes: React.ReactNode[] = [];
    let index = 0;

    while (index < renderContentSegments.length) {
      const segment = renderContentSegments[index];

      if (segment.type === 'tool_call') {
        if (hideInternalProcess) {
          while (index < renderContentSegments.length && renderContentSegments[index].type === 'tool_call') {
            index += 1;
          }
          continue;
        }

        const groupedToolCalls: ToolCall[] = [];
        const groupedToolCallIds = new Set<string>();
        let nextIndex = index;

        while (nextIndex < renderContentSegments.length && renderContentSegments[nextIndex].type === 'tool_call') {
          const toolCallId = normalizeMetaId(renderContentSegments[nextIndex]?.toolCallId);
          if (toolCallId && !groupedToolCallIds.has(toolCallId)) {
            groupedToolCallIds.add(toolCallId);
            const matchedToolCall = toolCallsById.get(toolCallId) || assistantToolCallsById?.get(toolCallId);

            if (matchedToolCall) {
              groupedToolCalls.push({
                ...matchedToolCall,
                id: toolCallId,
                messageId: matchedToolCall.messageId || message.id,
                name: matchedToolCall.name || 'unknown_tool',
                arguments: matchedToolCall.arguments ?? {},
                createdAt: matchedToolCall.createdAt || message.createdAt,
              } as ToolCall);
            } else {
              groupedToolCalls.push({
                id: toolCallId,
                messageId: message.id,
                name: 'unknown_tool',
                arguments: {},
                createdAt: message.createdAt,
              } as ToolCall);
            }
          }
          nextIndex += 1;
        }

        if (!collapseAssistantProcessByDefault && groupedToolCalls.length > 0) {
          nodes.push(
            <ToolCallTimeline
              key={`tool-group-${index}`}
              toolCalls={groupedToolCalls}
              toolResultById={toolResultById}
            />,
          );
        }

        index = nextIndex;
        continue;
      }

      if (segment.type === 'text') {
        const content = typeof segment.content === 'string' && segment.content.trim().length > 0
          ? segment.content
          : '';
        const nextIndex = index + 1;
        const shouldRenderStreamingCursor = isCurrentlyStreaming && nextIndex === renderContentSegments.length;

        if (content || shouldRenderStreamingCursor) {
          nodes.push(
            <div key={`segment-${index}`} className="prose prose-sm max-w-none">
              <MarkdownRenderer
                content={content}
                isStreaming={shouldRenderStreamingCursor}
                onApplyCode={onApplyCode}
              />
            </div>,
          );
        }

        index = nextIndex;
        continue;
      }

      if (segment.type === 'thinking') {
        if (hideInternalProcess) {
          index += 1;
          continue;
        }

        if (!collapseAssistantProcessByDefault) {
          nodes.push(
            <details
              key={`thinking-${index}`}
              className="group rounded-md border border-gray-200 bg-muted px-3 py-2 dark:border-gray-700"
            >
              <summary className="cursor-pointer select-none text-xs text-gray-500 dark:text-gray-400">
                Thinking
              </summary>
              <div className="mt-1">
                <MarkdownRenderer
                  content={segment.content || ''}
                  isStreaming={isCurrentlyStreaming && index === renderContentSegments.length - 1}
                  onApplyCode={onApplyCode}
                  className="thinking not-prose"
                />
              </div>
            </details>,
          );
        }

        index += 1;
        continue;
      }

      index += 1;
    }

    return <div className="space-y-0.5">{nodes}</div>;
  }

  return (
    <div className="space-y-0.5">
      {hasContent && (
        <div className="prose prose-sm max-w-none">
          <MarkdownRenderer
            content={message.content}
            isStreaming={isCurrentlyStreaming}
            onApplyCode={onApplyCode}
          />
        </div>
      )}

      {!hideInternalProcess && !collapseAssistantProcessByDefault && toolCalls.length > 0 && (
        <div className="space-y-0.5">
          <ToolCallTimeline
            toolCalls={toolCalls}
            toolResultById={toolResultById}
          />
        </div>
      )}
    </div>
  );
};

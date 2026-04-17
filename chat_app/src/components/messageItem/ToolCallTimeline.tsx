import React, { useMemo, useState } from 'react';
import type { Message, ToolCall } from '../../types';
import { getToolDisplayName } from '../../lib/tools/displayName';
import { cn } from '../../lib/utils';
import { ToolCallRenderer } from '../ToolCallRenderer';

interface ToolCallTimelineProps {
  toolCalls: ToolCall[];
  toolResultById?: Map<string, Message>;
}

export const ToolCallTimeline: React.FC<ToolCallTimelineProps> = ({
  toolCalls,
  toolResultById,
}) => {
  const [expanded, setExpanded] = useState(false);
  const shouldClampTimeline = toolCalls.length > 6;

  const resolveToolResult = (toolCall: ToolCall) => {
    if (toolCall.result !== undefined && toolCall.result !== null) {
      return toolCall.result;
    }
    const direct = toolResultById?.get(String(toolCall.id));
    if (direct?.content !== undefined && direct?.content !== null) {
      return direct.content;
    }
    return undefined;
  };

  const getToolStatus = (toolCall: ToolCall) => {
    if (toolCall.error) {
      return 'error';
    }
    const result = resolveToolResult(toolCall);
    if (result !== undefined && result !== null) {
      return 'success';
    }
    return 'pending';
  };

  const summaryStatus = useMemo(() => {
    let hasError = false;
    let allDone = true;

    toolCalls.forEach((toolCall) => {
      const status = getToolStatus(toolCall);
      if (status === 'error') {
        hasError = true;
      }
      if (status !== 'success') {
        allDone = false;
      }
    });

    if (hasError) {
      return 'error';
    }
    if (allDone) {
      return 'success';
    }
    return 'pending';
  }, [toolCalls, toolResultById]);

  const summaryNames = useMemo(() => {
    const names = toolCalls
      .map((toolCall) => toolCall?.name)
      .filter(Boolean)
      .map((name) => getToolDisplayName(String(name)));
    if (names.length === 0) {
      return '';
    }
    const shown = names.slice(0, 2).map((name) => `@${name}`).join(' · ');
    const more = names.length - 2;
    return more > 0 ? `${shown} · +${more}` : shown;
  }, [toolCalls]);

  const statusDotClass = summaryStatus === 'error'
    ? 'bg-red-500'
    : summaryStatus === 'success'
      ? 'bg-emerald-500'
      : 'bg-amber-500';

  return (
    <div className="space-y-1">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2 px-1 py-1 text-left text-xs text-muted-foreground hover:text-foreground"
        aria-label={expanded ? '收起工具时间线' : '展开工具时间线'}
        aria-expanded={expanded}
      >
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <span className={`inline-flex h-2 w-2 rounded-full ${statusDotClass}`} />
          <span className="font-medium text-foreground">工具调用</span>
          <span>· {toolCalls.length} 个</span>
          {summaryNames && (
            <span className="hidden truncate sm:inline">{summaryNames}</span>
          )}
          <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
            <span>{expanded ? '收起' : '展开'}</span>
            <svg
              className={`h-3 w-3 transition-transform ${expanded ? 'rotate-180' : ''}`}
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </span>
        </div>
      </button>

      {expanded && (
        <div
          className={cn(
            'space-y-1.5 pl-1',
            shouldClampTimeline && 'max-h-72 overflow-y-auto pr-1',
          )}
        >
          {toolCalls.map((toolCall, index) => {
            const status = getToolStatus(toolCall);
            const dotClass = status === 'error'
              ? 'bg-red-500'
              : status === 'success'
                ? 'bg-emerald-500'
                : 'bg-amber-500';

            return (
              <div key={toolCall.id || `tool-${index}`} className="flex gap-2.5">
                <div className="flex pt-2.5">
                  <span className={`h-1.5 w-1.5 rounded-full ${dotClass}`} />
                </div>
                <div className="min-w-0 flex-1">
                  <ToolCallRenderer
                    toolCall={toolCall}
                    toolResultById={toolResultById}
                  />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

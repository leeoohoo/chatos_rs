import React, { useEffect, useMemo, useState } from 'react';
import type { Message, ToolCall } from '../../types';
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
  const [activeToolIndex, setActiveToolIndex] = useState(0);

  useEffect(() => {
    if (toolCalls.length === 0) {
      setActiveToolIndex(0);
      return;
    }

    setActiveToolIndex((current) => (
      current >= toolCalls.length ? toolCalls.length - 1 : current
    ));
  }, [toolCalls]);

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
      .filter((name): name is string => Boolean(name));
    return names.slice(0, 3);
  }, [toolCalls]);

  const statusDotClass = summaryStatus === 'error'
    ? 'bg-red-500'
    : summaryStatus === 'success'
      ? 'bg-emerald-500'
      : 'bg-amber-500';
  const statusBadgeClass = summaryStatus === 'error'
    ? 'border-red-200 bg-red-50 text-red-700 dark:border-red-900/80 dark:bg-red-950/30 dark:text-red-300'
    : summaryStatus === 'success'
      ? 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900/80 dark:bg-emerald-950/30 dark:text-emerald-300'
      : 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/80 dark:bg-amber-950/30 dark:text-amber-300';
  const activeTool = toolCalls[activeToolIndex] || null;

  return (
    <div className="rounded-xl border border-border/70 bg-card/80 shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2.5">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
          <span
            className={cn(
              'inline-flex items-center gap-2 rounded-full border px-2.5 py-1 text-[11px] font-medium',
              statusBadgeClass,
            )}
          >
            <span className={`inline-flex h-2 w-2 rounded-full ${statusDotClass}`} />
            工具调用
          </span>
          <span className="inline-flex items-center rounded-full bg-muted px-2.5 py-1 text-[11px] text-muted-foreground">
            {toolCalls.length} 个
          </span>
          {summaryNames.map((name, index) => (
            <span
              key={`${name}-${index}`}
              className="hidden max-w-[180px] truncate rounded-full border border-border/60 bg-background/80 px-2.5 py-1 text-[11px] text-foreground/80 sm:inline-flex"
            >
              @{name}
            </span>
          ))}
          {toolCalls.length > summaryNames.length && (
            <span className="hidden rounded-full bg-muted px-2.5 py-1 text-[11px] text-muted-foreground sm:inline-flex">
              +{toolCalls.length - summaryNames.length}
            </span>
          )}
        </div>
        <button
          type="button"
          onClick={() => setExpanded((prev) => !prev)}
          className="inline-flex items-center gap-1 rounded-full border border-border/70 bg-background px-2.5 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label={expanded ? '收起工具时间线' : '展开工具时间线'}
          aria-expanded={expanded}
        >
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
        </button>
      </div>

      {expanded && (
        <div className="border-t border-border/60 px-3 pb-3 pt-3">
          <div className="mb-3 flex flex-wrap gap-2">
            {toolCalls.map((toolCall, index) => {
              const status = getToolStatus(toolCall);
              const dotClass = status === 'error'
                ? 'bg-red-500'
                : status === 'success'
                  ? 'bg-emerald-500'
                  : 'bg-amber-500';
              const active = index === activeToolIndex;

              return (
                <button
                  key={toolCall.id || `tool-${index}`}
                  type="button"
                  onClick={() => setActiveToolIndex(index)}
                  className={cn(
                    'inline-flex max-w-full items-center gap-2 rounded-full border px-3 py-1.5 text-xs transition-colors',
                    active
                      ? 'border-primary/30 bg-primary/10 text-primary shadow-sm'
                      : 'border-border/70 bg-background text-muted-foreground hover:border-primary/20 hover:text-foreground',
                  )}
                  aria-pressed={active}
                >
                  <span className={`h-2 w-2 rounded-full ${dotClass}`} />
                  <span className="text-[11px] text-muted-foreground">#{index + 1}</span>
                  <span className="max-w-[220px] truncate font-medium">
                    @{toolCall.name || 'unknown_tool'}
                  </span>
                </button>
              );
            })}
          </div>

          {activeTool && (
            <div className="rounded-2xl border border-border/60 bg-muted/20 p-2">
              <ToolCallRenderer
                toolCall={activeTool}
                toolResultById={toolResultById}
                className="w-full"
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
};

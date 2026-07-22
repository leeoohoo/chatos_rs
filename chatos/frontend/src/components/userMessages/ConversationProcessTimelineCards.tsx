// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useState } from 'react';
import {
  AlertTriangle,
  Bot,
  ChevronDown,
  CheckCircle2,
  Clock,
} from 'lucide-react';

import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { cn } from '../../lib/utils';
import {
  buildDisplayValue,
  formatTime,
  type TimelineItem,
  type TimelineStatus,
} from './ConversationProcessTimelineModel';
import {
  buildToolActionSummary,
  toolActionText,
} from './ConversationProcessToolSummary';

export const SummaryPill: React.FC<{ label: string; value: number }> = ({ label, value }) => (
  <div className="rounded-md border border-border bg-background px-3 py-2">
    <div className="text-[11px] text-muted-foreground">{label}</div>
    <div className="mt-0.5 text-base font-semibold text-foreground">{value}</div>
  </div>
);

const ValueSection: React.FC<{
  emptyArrayText: string;
  emptyObjectText: string;
  emptyText: string;
  title: string;
  value: unknown;
}> = ({
  emptyArrayText,
  emptyObjectText,
  emptyText,
  title,
  value,
}) => {
  const displayValue = buildDisplayValue(value, {
    emptyArrayText,
    emptyObjectText,
    emptyText,
  });

  return (
    <section className="min-w-0 overflow-hidden rounded-md border border-border/80 bg-muted/15">
      <div className="border-b border-border/70 px-3 py-1.5 text-[11px] font-medium text-muted-foreground">
        {title}
      </div>
      <div className="p-3">
        {displayValue.kind === 'empty' ? (
          <div className="text-sm text-muted-foreground">{displayValue.text}</div>
        ) : displayValue.kind === 'json' ? (
          <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-5 text-foreground">
            {displayValue.text}
          </pre>
        ) : (
          <div className="max-h-80 overflow-auto break-words">
            <LazyMarkdownRenderer content={displayValue.text} className="text-sm" />
          </div>
        )}
      </div>
    </section>
  );
};

export const TimelineDot: React.FC<{
  status?: TimelineStatus;
  type: TimelineItem['type'];
}> = ({ status, type }) => {
  const Icon = type === 'model'
    ? Bot
    : status === 'error'
      ? AlertTriangle
      : status === 'completed'
        ? CheckCircle2
        : Clock;

  return (
    <span className={cn(
      'absolute left-0 top-3 flex h-7 w-7 items-center justify-center rounded-full border bg-background shadow-sm',
      status === 'error'
        ? 'border-destructive/30 text-destructive'
        : status === 'completed'
          ? 'border-emerald-200 text-emerald-600 dark:border-emerald-800 dark:text-emerald-300'
          : type === 'model'
            ? 'border-border text-primary'
            : 'border-amber-200 text-amber-600 dark:border-amber-800 dark:text-amber-300',
    )}
    >
      <Icon className="h-3.5 w-3.5" />
    </span>
  );
};

const ToolCallCard: React.FC<{
  item: Extract<TimelineItem, { type: 'tool_call' }>;
}> = ({ item }) => {
  const [expanded, setExpanded] = useState(false);
  const actionSummary = buildToolActionSummary(
    item.toolCall.name || '',
    item.toolCall.arguments,
  );
  const actionText = toolActionText(actionSummary, item.status);

  return (
    <article className="overflow-hidden rounded-md border border-border/80 bg-background">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-accent/40"
        aria-expanded={expanded}
        onClick={() => setExpanded((prev) => !prev)}
      >
        <span className={cn(
          'min-w-0 flex-1 text-sm',
          item.status === 'error' ? 'text-destructive' : 'text-foreground',
        )}
        >
          {actionText}
        </span>
        <span className="hidden shrink-0 text-[11px] text-muted-foreground sm:inline">
          {formatTime(item.createdAt)}
        </span>
        <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
          {expanded ? '收起' : '展开'}
          <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')} />
        </span>
      </button>

      {expanded ? (
        <div className="grid gap-3 border-t border-border/70 bg-muted/10 p-3 lg:grid-cols-[minmax(0,0.85fr)_minmax(0,1.15fr)]">
          <ValueSection
            title="主要参数"
            value={item.toolCall.arguments}
            emptyText="无参数"
            emptyArrayText="无参数"
            emptyObjectText="无参数"
          />
          {item.error ? (
            <ValueSection
              title="错误"
              value={item.error}
              emptyText="工具返回错误"
              emptyArrayText="工具返回错误"
              emptyObjectText="工具返回错误"
            />
          ) : (
            <ValueSection
              title="返回结果"
              value={item.hasResult ? item.result : undefined}
              emptyText={item.hasResult ? '返回为空内容' : '暂无返回'}
              emptyArrayText="返回为空数组"
              emptyObjectText="返回为空对象"
            />
          )}
        </div>
      ) : null}
    </article>
  );
};

const ToolResultCard: React.FC<{
  item: Extract<TimelineItem, { type: 'tool_result' }>;
}> = ({ item }) => {
  const [expanded, setExpanded] = useState(false);
  const resultValue = item.error || item.result;

  return (
    <article className="overflow-hidden rounded-md border border-border/80 bg-background">
      <button
        type="button"
        className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-accent/40"
        aria-expanded={expanded}
        onClick={() => setExpanded((prev) => !prev)}
      >
        <span className={cn(
          'min-w-0 flex-1 text-sm',
          item.error ? 'text-destructive' : 'text-foreground',
        )}
        >
          {item.error ? '执行结果返回失败' : '已收到执行结果'}
        </span>
        <span className="hidden shrink-0 text-[11px] text-muted-foreground sm:inline">
          {formatTime(item.createdAt)}
        </span>
        <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
          {expanded ? '收起' : '展开'}
          <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')} />
        </span>
      </button>
      {expanded ? (
        <div className="border-t border-border/70 bg-muted/10 p-3">
          <ValueSection
            title={item.error ? '错误' : '返回结果'}
            value={resultValue}
            emptyText={item.hasResult ? '返回为空内容' : '暂无返回'}
            emptyArrayText="返回为空数组"
            emptyObjectText="返回为空对象"
          />
        </div>
      ) : null}
    </article>
  );
};

const ModelCard: React.FC<{
  item: Extract<TimelineItem, { type: 'model' }>;
}> = ({ item }) => (
  <article className="rounded-md border border-border/80 bg-background px-3.5 py-3">
    <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
      <span className="inline-flex items-center gap-1 rounded border border-border bg-muted/40 px-2 py-0.5 font-medium text-foreground">
        <Bot className="h-3 w-3" />
        {item.label}
      </span>
      <span className="text-muted-foreground">{formatTime(item.createdAt)}</span>
    </div>
    <div className="rounded-md border border-border/80 bg-muted/15 p-3">
      <LazyMarkdownRenderer content={item.content} className="text-sm" />
    </div>
  </article>
);

export const renderTimelineCard = (item: TimelineItem) => {
  if (item.type === 'model') {
    return <ModelCard item={item} />;
  }
  if (item.type === 'tool_result') {
    return <ToolResultCard item={item} />;
  }
  return <ToolCallCard item={item} />;
};

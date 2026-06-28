import React, { useState } from 'react';
import {
  AlertTriangle,
  Bot,
  ChevronDown,
  CheckCircle2,
  Clock,
  Hammer,
} from 'lucide-react';

import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { getToolDisplayName } from '../../lib/tools/displayName';
import { cn } from '../../lib/utils';
import {
  buildDisplayValue,
  buildValueSummary,
  formatTime,
  type TimelineItem,
  type TimelineStatus,
} from './ConversationProcessTimelineModel';

const statusLabel = (status: TimelineStatus): string => {
  if (status === 'error') {
    return '错误';
  }
  if (status === 'completed') {
    return '已返回';
  }
  return '等待返回';
};

const statusClassName = (status: TimelineStatus): string => {
  if (status === 'error') {
    return 'border-destructive/30 bg-destructive/10 text-destructive';
  }
  if (status === 'completed') {
    return 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-300';
  }
  return 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300';
};

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
  index: number;
  item: Extract<TimelineItem, { type: 'tool_call' }>;
}> = ({ index, item }) => {
  const [expanded, setExpanded] = useState(false);
  const rawToolName = item.toolCall.name || 'unknown_tool';
  const displayToolName = getToolDisplayName(rawToolName);
  const showRawName = rawToolName !== displayToolName;
  const parameterSummary = buildValueSummary(item.toolCall.arguments, {
    emptyArrayText: '无参数',
    emptyObjectText: '无参数',
    emptyText: '无参数',
  });
  const resultValue = item.error || (item.hasResult ? item.result : undefined);
  const resultSummary = buildValueSummary(resultValue, {
    emptyArrayText: item.error ? '工具返回错误' : '返回为空数组',
    emptyObjectText: item.error ? '工具返回错误' : '返回为空对象',
    emptyText: item.error ? '工具返回错误' : (item.hasResult ? '返回为空内容' : '暂无返回'),
  });

  return (
    <article className="rounded-lg border border-border bg-background px-3.5 py-3 shadow-sm">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span className="inline-flex items-center gap-1 rounded border border-border bg-muted/40 px-2 py-0.5 font-medium text-foreground">
              <Hammer className="h-3 w-3" />
              {index}. 工具调用
            </span>
            <span className="rounded-md border border-primary/25 bg-primary/10 px-2 py-0.5 font-mono text-[11px] font-semibold text-primary">
              {displayToolName}
            </span>
            <span className="text-muted-foreground">{formatTime(item.createdAt)}</span>
          </div>
          <div className="mt-1.5 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            <span>
              调用 ID <code className="rounded bg-muted px-1 py-0.5 font-mono">{item.toolCall.id || '-'}</code>
            </span>
            {showRawName ? (
              <span>
                原始名称 <code className="rounded bg-muted px-1 py-0.5 font-mono">{rawToolName}</code>
              </span>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className={cn(
            'inline-flex items-center rounded-full border px-2 py-0.5 text-[11px] font-medium',
            statusClassName(item.status),
          )}
          >
            {statusLabel(item.status)}
          </span>
          <button
            type="button"
            className="inline-flex items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-[11px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
            aria-expanded={expanded}
            onClick={() => setExpanded((prev) => !prev)}
          >
            {expanded ? '收起' : '展开'}
            <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')} />
          </button>
        </div>
      </div>

      <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
        <span className="rounded-md border border-border/70 bg-muted/20 px-2 py-1">
          参数：{parameterSummary}
        </span>
        <span className={cn(
          'rounded-md border px-2 py-1',
          item.error
            ? 'border-destructive/25 bg-destructive/10 text-destructive'
            : 'border-border/70 bg-muted/20',
        )}
        >
          {item.error ? '错误' : '返回'}：{resultSummary}
        </span>
      </div>

      {expanded ? (
        <div className="mt-3 grid gap-3 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
          <ValueSection
            title="参数"
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
  index: number;
  item: Extract<TimelineItem, { type: 'tool_result' }>;
}> = ({ index, item }) => {
  const [expanded, setExpanded] = useState(false);
  const resultValue = item.error || item.result;
  const resultSummary = buildValueSummary(resultValue, {
    emptyArrayText: item.error ? '工具返回错误' : '返回为空数组',
    emptyObjectText: item.error ? '工具返回错误' : '返回为空对象',
    emptyText: item.error ? '工具返回错误' : (item.hasResult ? '返回为空内容' : '暂无返回'),
  });

  return (
    <article className="rounded-lg border border-border bg-background px-3.5 py-3 shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="inline-flex items-center gap-1 rounded border border-border bg-muted/40 px-2 py-0.5 font-medium text-foreground">
            <Hammer className="h-3 w-3" />
            {index}. 未匹配工具返回
          </span>
          <span className="text-muted-foreground">{formatTime(item.createdAt)}</span>
        </div>
        <span className={cn(
          'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 text-[11px] font-medium',
          statusClassName(item.status),
        )}
        >
          {statusLabel(item.status)}
        </span>
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-[11px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-expanded={expanded}
          onClick={() => setExpanded((prev) => !prev)}
        >
          {expanded ? '收起' : '展开'}
          <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')} />
        </button>
      </div>
      {item.callId ? (
        <div className="mt-1.5 text-[11px] text-muted-foreground">
          调用 ID <code className="rounded bg-muted px-1 py-0.5 font-mono">{item.callId}</code>
        </div>
      ) : null}
      <div className={cn(
        'mt-3 inline-flex rounded-md border px-2 py-1 text-xs',
        item.error
          ? 'border-destructive/25 bg-destructive/10 text-destructive'
          : 'border-border/70 bg-muted/20 text-muted-foreground',
      )}
      >
        {item.error ? '错误' : '返回'}：{resultSummary}
      </div>
      {expanded ? (
        <div className="mt-3">
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
  index: number;
  item: Extract<TimelineItem, { type: 'model' }>;
}> = ({ index, item }) => (
  <article className="rounded-lg border border-border bg-background px-3.5 py-3 shadow-sm">
    <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
      <span className="inline-flex items-center gap-1 rounded border border-border bg-muted/40 px-2 py-0.5 font-medium text-foreground">
        <Bot className="h-3 w-3" />
        {index}. {item.label}
      </span>
      <span className="text-muted-foreground">{formatTime(item.createdAt)}</span>
    </div>
    <div className="rounded-md border border-border/80 bg-muted/15 p-3">
      <LazyMarkdownRenderer content={item.content} className="text-sm" />
    </div>
  </article>
);

export const renderTimelineCard = (item: TimelineItem, index: number) => {
  if (item.type === 'model') {
    return <ModelCard item={item} index={index + 1} />;
  }
  if (item.type === 'tool_result') {
    return <ToolResultCard item={item} index={index + 1} />;
  }
  return <ToolCallCard item={item} index={index + 1} />;
};

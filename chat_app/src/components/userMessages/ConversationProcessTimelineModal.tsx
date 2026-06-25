import React, { useMemo, useState } from 'react';
import {
  AlertTriangle,
  Bot,
  ChevronDown,
  CheckCircle2,
  Clock,
  Hammer,
  X,
} from 'lucide-react';

import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import {
  getMessageMetadataToolCalls,
  getMessageToolResultCallId,
  type MessageToolCallLike,
} from '../messageItem/messageReaders';
import { getToolDisplayName } from '../../lib/tools/displayName';
import { cn } from '../../lib/utils';
import type { Message } from '../../types';
import type { UserMessageTurn } from './types';

interface ConversationProcessTimelineModalProps {
  item: UserMessageTurn | null;
  messages: Message[];
  loading: boolean;
  error: string | null;
  onClose: () => void;
}

const readRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const hasOwn = (record: Record<string, unknown>, key: string): boolean => (
  Object.prototype.hasOwnProperty.call(record, key)
);

const isDefined = (value: unknown): boolean => value !== undefined && value !== null;

const formatTime = (date: Date): string => {
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) {
    return '-';
  }
  return date.toLocaleString();
};

const isProcessMessage = (message: Message): boolean => {
  const metadata = readRecord(message.metadata);
  if (metadata?.historyProcessLoaded === true) {
    return true;
  }
  if (metadata?.historyProcessPlaceholder === false && readString(metadata?.historyProcessUserMessageId)) {
    return true;
  }
  return false;
};

const readTaskRunnerMessageKind = (message: Message): string => {
  const metadata = readRecord(message.metadata);
  const taskRunnerAsync = readRecord(metadata?.task_runner_async);
  return readString(taskRunnerAsync?.message_kind);
};

const processLabel = (message: Message): string => {
  const messageKind = readTaskRunnerMessageKind(message);
  if (messageKind === 'task_terminal_update') {
    return '任务状态更新';
  }
  if (message.role === 'tool') {
    return '工具结果';
  }
  if (message.role === 'assistant') {
    return '模型过程';
  }
  return message.role;
};

type TextSegment = {
  content: string;
  type: string;
};

const readContentSegments = (message: Message): TextSegment[] => {
  const metadata = readRecord(message.metadata);
  const rawSegments = Array.isArray(metadata?.contentSegments)
    ? metadata.contentSegments
    : Array.isArray(metadata?.content_segments)
      ? metadata.content_segments
      : [];

  return rawSegments
    .map((segment) => {
      const record = readRecord(segment);
      const content = readString(record?.content);
      const type = readString(record?.type);
      return content ? { content, type } : null;
    })
    .filter((segment): segment is TextSegment => segment !== null);
};

const processContent = (message: Message): string => {
  const segmentText = readContentSegments(message)
    .filter((segment) => segment.type === 'text' || segment.type === 'thinking')
    .map((segment) => segment.content)
    .filter(Boolean)
    .join('\n\n');
  if (segmentText) {
    return segmentText;
  }
  return readString(message.content);
};

const readToolMessageResult = (message: Message): unknown => {
  const metadata = readRecord(message.metadata);
  if (metadata && hasOwn(metadata, 'structured_result')) {
    return metadata.structured_result;
  }
  if (metadata && hasOwn(metadata, 'structuredResult')) {
    return metadata.structuredResult;
  }
  return message.content;
};

const readToolMessageError = (message: Message | undefined): string => {
  if (!message) {
    return '';
  }
  const metadata = readRecord(message.metadata);
  const isError = metadata?.isError === true || metadata?.is_error === true;
  return isError ? readString(message.content) || '工具返回错误' : '';
};

type TimelineStatus = 'completed' | 'error' | 'pending';

type TimelineItem =
  | {
    content: string;
    createdAt: Date;
    id: string;
    label: string;
    type: 'model';
  }
  | {
    createdAt: Date;
    error: string;
    hasResult: boolean;
    id: string;
    result: unknown;
    resultMessage?: Message;
    status: TimelineStatus;
    toolCall: MessageToolCallLike;
    type: 'tool_call';
  }
  | {
    callId: string;
    createdAt: Date;
    error: string;
    hasResult: boolean;
    id: string;
    result: unknown;
    status: TimelineStatus;
    type: 'tool_result';
  };

const resolveToolCallResult = (
  toolCall: MessageToolCallLike,
  resultMessage: Message | undefined,
): { hasResult: boolean; result: unknown } => {
  if (isDefined(toolCall.finalResult)) {
    return { hasResult: true, result: toolCall.finalResult };
  }
  if (isDefined(toolCall.result)) {
    return { hasResult: true, result: toolCall.result };
  }
  if (resultMessage) {
    return { hasResult: true, result: readToolMessageResult(resultMessage) };
  }
  return { hasResult: false, result: undefined };
};

const buildTimelineItems = (processMessages: Message[]): TimelineItem[] => {
  const toolResultByCallId = new Map<string, Message>();
  const knownToolCallIds = new Set<string>();

  processMessages.forEach((message) => {
    if (message.role === 'tool') {
      const toolCallId = getMessageToolResultCallId(message);
      if (toolCallId && !toolResultByCallId.has(toolCallId)) {
        toolResultByCallId.set(toolCallId, message);
      }
      return;
    }

    getMessageMetadataToolCalls(message).forEach((toolCall) => {
      if (toolCall.id) {
        knownToolCallIds.add(toolCall.id);
      }
    });
  });

  return processMessages.flatMap((message): TimelineItem[] => {
    if (message.role === 'tool') {
      const callId = getMessageToolResultCallId(message);
      if (callId && knownToolCallIds.has(callId)) {
        return [];
      }
      const error = readToolMessageError(message);
      return [{
        callId,
        createdAt: message.createdAt,
        error,
        hasResult: true,
        id: `tool-result-${message.id}`,
        result: readToolMessageResult(message),
        status: error ? 'error' : 'completed',
        type: 'tool_result',
      }];
    }

    const items: TimelineItem[] = [];
    const content = processContent(message);
    if (content) {
      items.push({
        content,
        createdAt: message.createdAt,
        id: `model-${message.id}`,
        label: processLabel(message),
        type: 'model',
      });
    }

    getMessageMetadataToolCalls(message)
      .filter((toolCall) => toolCall.id || toolCall.name)
      .forEach((toolCall, index) => {
        const resultMessage = toolCall.id ? toolResultByCallId.get(toolCall.id) : undefined;
        const { hasResult, result } = resolveToolCallResult(toolCall, resultMessage);
        const error = toolCall.error || readToolMessageError(resultMessage);
        const status: TimelineStatus = error
          ? 'error'
          : (hasResult || toolCall.completed === true ? 'completed' : 'pending');

        items.push({
          createdAt: toolCall.createdAt || message.createdAt,
          error,
          hasResult,
          id: `tool-call-${message.id}-${toolCall.id || index}`,
          result,
          resultMessage,
          status,
          toolCall,
          type: 'tool_call',
        });
      });

    return items;
  });
};

type DisplayValue = {
  kind: 'empty' | 'json' | 'text';
  text: string;
};

const parseJsonText = (value: string): { parsed: boolean; value: unknown } => {
  const trimmed = value.trim();
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) {
    return { parsed: false, value };
  }
  try {
    return { parsed: true, value: JSON.parse(trimmed) };
  } catch {
    return { parsed: false, value };
  }
};

const stringifyValue = (value: unknown): string => {
  try {
    const formatted = JSON.stringify(value, null, 2);
    return formatted ?? String(value);
  } catch {
    return String(value);
  }
};

const clipSummary = (value: string, limit = 96): string => {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > limit ? `${normalized.slice(0, limit)}...` : normalized;
};

const buildDisplayValue = (
  value: unknown,
  options: {
    emptyArrayText: string;
    emptyObjectText: string;
    emptyText: string;
  },
): DisplayValue => {
  if (value === undefined || value === null) {
    return { kind: 'empty', text: options.emptyText };
  }

  let normalized: unknown = value;
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) {
      return { kind: 'empty', text: options.emptyText };
    }
    const parsed = parseJsonText(trimmed);
    if (!parsed.parsed) {
      return { kind: 'text', text: trimmed };
    }
    normalized = parsed.value;
  }

  if (Array.isArray(normalized)) {
    if (normalized.length === 0) {
      return { kind: 'empty', text: options.emptyArrayText };
    }
    return { kind: 'json', text: stringifyValue(normalized) };
  }

  const record = readRecord(normalized);
  if (record) {
    if (Object.keys(record).length === 0) {
      return { kind: 'empty', text: options.emptyObjectText };
    }
    return { kind: 'json', text: stringifyValue(record) };
  }

  return { kind: 'text', text: String(normalized) };
};

const buildValueSummary = (
  value: unknown,
  options: {
    emptyArrayText: string;
    emptyObjectText: string;
    emptyText: string;
  },
): string => {
  if (value === undefined || value === null) {
    return options.emptyText;
  }

  let normalized: unknown = value;
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) {
      return options.emptyText;
    }
    const parsed = parseJsonText(trimmed);
    if (!parsed.parsed) {
      return clipSummary(trimmed);
    }
    normalized = parsed.value;
  }

  if (Array.isArray(normalized)) {
    return normalized.length === 0 ? options.emptyArrayText : `数组 ${normalized.length} 项`;
  }

  const record = readRecord(normalized);
  if (record) {
    const fieldCount = Object.keys(record).length;
    return fieldCount === 0 ? options.emptyObjectText : `对象 ${fieldCount} 个字段`;
  }

  return clipSummary(String(normalized));
};

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

const SummaryPill: React.FC<{ label: string; value: number }> = ({ label, value }) => (
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

const TimelineDot: React.FC<{
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

const renderTimelineCard = (item: TimelineItem, index: number) => {
  if (item.type === 'model') {
    return <ModelCard item={item} index={index + 1} />;
  }
  if (item.type === 'tool_result') {
    return <ToolResultCard item={item} index={index + 1} />;
  }
  return <ToolCallCard item={item} index={index + 1} />;
};

export const ConversationProcessTimelineModal: React.FC<ConversationProcessTimelineModalProps> = ({
  item,
  messages,
  loading,
  error,
  onClose,
}) => {
  const processMessages = useMemo(
    () => messages.filter(isProcessMessage),
    [messages],
  );
  const timelineItems = useMemo(
    () => buildTimelineItems(processMessages),
    [processMessages],
  );
  const summary = useMemo(() => timelineItems.reduce(
    (acc, timelineItem) => {
      if (timelineItem.type === 'model') {
        acc.model += 1;
      }
      if (timelineItem.type === 'tool_call') {
        acc.toolCall += 1;
        if (timelineItem.hasResult) {
          acc.toolResult += 1;
        }
      }
      if (timelineItem.type === 'tool_result') {
        acc.toolResult += 1;
      }
      if ('status' in timelineItem && timelineItem.status === 'error') {
        acc.error += 1;
      }
      return acc;
    },
    { error: 0, model: 0, toolCall: 0, toolResult: 0 },
  ), [timelineItems]);

  if (!item) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[70]">
      <button
        type="button"
        className="absolute inset-0 bg-black/45"
        aria-label="关闭"
        onClick={onClose}
      />
      <div className="absolute left-1/2 top-1/2 flex max-h-[88vh] w-[calc(100vw-24px)] max-w-5xl -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl sm:w-[calc(100vw-40px)]">
        <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-foreground">执行过程</h2>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {formatTime(item.userMessage.createdAt)} · {item.processMessageCount} 条过程消息 · {timelineItems.length} 个事件
            </p>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {loading ? (
            <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
              正在加载执行过程...
            </div>
          ) : error ? (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-3 text-sm text-destructive">
              {error}
            </div>
          ) : timelineItems.length === 0 ? (
            <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
              暂无可展示的过程消息。
            </div>
          ) : (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                <SummaryPill label="模型过程" value={summary.model} />
                <SummaryPill label="工具调用" value={summary.toolCall} />
                <SummaryPill label="工具返回" value={summary.toolResult} />
                <SummaryPill label="错误" value={summary.error} />
              </div>
              <ol className="relative space-y-3 before:absolute before:bottom-4 before:left-[13px] before:top-4 before:w-px before:bg-border">
                {timelineItems.map((timelineItem, index) => (
                  <li key={timelineItem.id} className="relative pl-9">
                    <TimelineDot
                      type={timelineItem.type}
                      status={'status' in timelineItem ? timelineItem.status : undefined}
                    />
                    {renderTimelineCard(timelineItem, index)}
                  </li>
                ))}
              </ol>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

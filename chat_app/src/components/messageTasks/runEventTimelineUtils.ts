import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';

type TimelineTone = 'danger' | 'info' | 'muted' | 'success' | 'warning';

export interface RunEventTimelineEntry {
  key: string;
  kind: 'group' | 'single';
  eventType: string;
  title: string;
  tone: TimelineTone;
  events: MessageTaskRunnerRunEvent[];
  startedAt?: string | null;
  finishedAt?: string | null;
  message?: string | null;
  aggregatedText?: string | null;
  payload?: unknown;
  payloadSummary?: string | null;
  summary?: string | null;
}

const GROUPED_EVENT_TYPES = new Set(['chunk', 'thinking', 'tool_stream']);

export function buildRunEventTimelineEntries(
  events: MessageTaskRunnerRunEvent[],
): RunEventTimelineEntry[] {
  const entries: RunEventTimelineEntry[] = [];

  for (let index = 0; index < events.length;) {
    const event = events[index];
    const eventType = normalizeEventType(event.event_type);

    if (GROUPED_EVENT_TYPES.has(eventType)) {
      const groupKey = buildGroupingKey(event);
      const group: MessageTaskRunnerRunEvent[] = [event];
      index += 1;

      while (index < events.length) {
        const nextEvent = events[index];
        if (normalizeEventType(nextEvent.event_type) !== eventType) {
          break;
        }
        if (buildGroupingKey(nextEvent) !== groupKey) {
          break;
        }
        group.push(nextEvent);
        index += 1;
      }

      entries.push(buildGroupedEntry(eventType, group));
      continue;
    }

    entries.push(buildSingleEntry(event));
    index += 1;
  }

  return entries;
}

export function describeStructuredValueSummary(value: unknown): string {
  if (value === null || value === undefined || value === '') {
    return '空内容';
  }
  if (typeof value === 'string') {
    return `${value.length} 字文本`;
  }
  if (Array.isArray(value)) {
    return `${value.length} 项`;
  }
  if (typeof value === 'object') {
    return `${Object.keys(value as Record<string, unknown>).length} 个字段`;
  }
  return '标量值';
}

function buildSingleEntry(event: MessageTaskRunnerRunEvent): RunEventTimelineEntry {
  const eventType = normalizeEventType(event.event_type);
  return {
    key: event.id,
    kind: 'single',
    eventType,
    title: describeEventTitle(event),
    tone: describeEventTone(eventType),
    events: [event],
    startedAt: event.created_at,
    finishedAt: event.created_at,
    message: readString(event.message),
    payload: event.payload,
    payloadSummary: event.payload === undefined ? null : describeStructuredValueSummary(event.payload),
    summary: describeSingleEventSummary(event),
  };
}

function buildGroupedEntry(
  eventType: string,
  events: MessageTaskRunnerRunEvent[],
): RunEventTimelineEntry {
  const aggregatedText = events
    .map((event) => extractEventText(event))
    .filter((value): value is string => Boolean(value))
    .join('\n\n');
  const charCount = aggregatedText.length;
  const firstEvent = events[0];
  const lastEvent = events[events.length - 1];
  const rawMessage = readString(firstEvent.message);
  const message = rawMessage && rawMessage !== aggregatedText ? rawMessage : null;

  return {
    key: `${firstEvent.id}:${eventType}:${events.length}`,
    kind: 'group',
    eventType,
    title: describeEventTitle(firstEvent),
    tone: describeEventTone(eventType),
    events,
    startedAt: firstEvent.created_at,
    finishedAt: lastEvent.created_at,
    message,
    aggregatedText: aggregatedText || null,
    summary: describeGroupedSummary(eventType, events.length, charCount),
  };
}

function normalizeEventType(value: unknown): string {
  return readString(value)?.toLowerCase() || 'unknown';
}

function buildGroupingKey(event: MessageTaskRunnerRunEvent): string {
  const eventType = normalizeEventType(event.event_type);
  if (eventType !== 'tool_stream') {
    return eventType;
  }

  const payload = asRecord(event.payload);
  return [
    eventType,
    readString(payload?.tool_call_id) || '',
    readString(payload?.name) || '',
  ].join(':');
}

function describeEventTitle(event: MessageTaskRunnerRunEvent): string {
  const eventType = normalizeEventType(event.event_type);
  if (eventType === 'queued') {
    return '任务入队';
  }
  if (eventType === 'running') {
    return '任务开始执行';
  }
  if (eventType === 'model_request') {
    return '模型请求';
  }
  if (eventType === 'tools_start') {
    return '开始调用工具';
  }
  if (eventType === 'tool_stream') {
    const payload = asRecord(event.payload);
    const toolName = readString(payload?.name);
    return toolName ? `工具输出 · ${toolName}` : '工具输出';
  }
  if (eventType === 'chunk') {
    return '模型回复';
  }
  if (eventType === 'thinking') {
    return '思考过程';
  }
  if (eventType === 'completed' || eventType === 'succeeded' || eventType === 'success') {
    return '运行完成';
  }
  if (eventType === 'failed' || eventType === 'error') {
    return '运行失败';
  }
  if (eventType === 'cancelled' || eventType === 'canceled') {
    return '运行取消';
  }

  return eventType.replace(/_/g, ' ');
}

function describeEventTone(eventType: string): TimelineTone {
  if (eventType.includes('failed') || eventType.includes('error')) {
    return 'danger';
  }
  if (eventType.includes('completed') || eventType.includes('succeeded') || eventType === 'success') {
    return 'success';
  }
  if (eventType.includes('cancel')) {
    return 'muted';
  }
  if (eventType === 'queued' || eventType === 'running') {
    return 'warning';
  }
  return 'info';
}

function describeSingleEventSummary(event: MessageTaskRunnerRunEvent): string | null {
  const eventType = normalizeEventType(event.event_type);
  if (eventType === 'model_request' && event.payload !== undefined) {
    return `payload ${describeStructuredValueSummary(event.payload)}`;
  }
  if (eventType === 'tools_start' && Array.isArray(event.payload)) {
    return `${event.payload.length} 个工具调用`;
  }
  if (eventType === 'tool_stream') {
    const payload = asRecord(event.payload);
    const status = payload?.success === false || payload?.is_error === true ? '失败' : '完成';
    return `${status}${payload?.result !== undefined ? ` · ${describeStructuredValueSummary(payload.result)}` : ''}`;
  }
  return null;
}

function describeGroupedSummary(eventType: string, count: number, charCount: number): string {
  if (eventType === 'tool_stream') {
    return count > 1 ? `已聚合 ${count} 条工具输出` : '默认折叠';
  }
  if (count > 1) {
    return `已聚合 ${count} 段内容${charCount > 0 ? ` · ${charCount} 字` : ''}`;
  }
  return charCount > 0 ? `${charCount} 字内容` : '默认折叠';
}

function extractEventText(event: MessageTaskRunnerRunEvent): string | null {
  const payload = asRecord(event.payload);
  return (
    readString(payload?.text)
    || readString(payload?.chunk)
    || readString(payload?.content)
    || readString(payload?.output)
    || readString(event.message)
    || null
  );
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function readString(value: unknown): string | null {
  if (typeof value !== 'string') {
    return null;
  }
  const text = value.trim();
  return text ? text : null;
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Space, Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRunEventRecord } from '../../types';
import { payloadAsOptionalNumber, payloadAsOptionalString, payloadAsRecord } from './payloadUtils';
import { CollapsiblePayload } from './payloadView';


export type ToolCallView = {
  invocationId: string;
  callId: string;
  name: string;
  arguments?: unknown;
};

export type ToolResultView = {
  invocationId: string;
  toolCallId: string;
  name: string;
  success: boolean;
  content: string;
  result?: unknown;
};

export function collectToolCalls(
  events: TaskRunEventRecord[],
  report: unknown,
): ToolCallView[] {
  const fromEvents = events
    .filter((event) => event.event_type === 'tools_start')
    .flatMap((event) => extractToolCallArray(event.payload));
  if (fromEvents.length) {
    return dedupeToolCalls(fromEvents);
  }
  const reportToolCalls = payloadAsRecord(report)?.tool_calls;
  return dedupeToolCalls(extractToolCallArray(reportToolCalls));
}

export function collectToolResults(events: TaskRunEventRecord[]): ToolResultView[] {
  const results = events
    .filter((event) => event.event_type === 'tool_stream')
    .map((event) => payloadAsRecord(event.payload))
    .filter((payload): payload is Record<string, unknown> => Boolean(payload))
    .map((payload) => ({
      invocationId: payloadAsOptionalString(payload.invocation_id) || '',
      toolCallId: payloadAsOptionalString(payload.tool_call_id) || '',
      name: payloadAsOptionalString(payload.name) || 'unknown_tool',
      success: Boolean(payload.success) && !Boolean(payload.is_error),
      content: payloadAsOptionalString(payload.content) || '',
      result: payload.result,
      isStream: Boolean(payload.is_stream),
    }));
  const byInvocation = new Map<string, (typeof results)[number]>();
  results.forEach((result) => {
    const key = result.invocationId || `${result.toolCallId}::${result.name}`;
    const current = byInvocation.get(key);
    if (!current || current.isStream || !result.isStream) {
      byInvocation.set(key, result);
    }
  });
  return Array.from(byInvocation.values()).map(({ isStream: _isStream, ...result }) => result);
}

export function summarizeStreamEvents(events: TaskRunEventRecord[]) {
  let chunkCount = 0;
  let chunkChars = 0;
  let thinkingCount = 0;
  let thinkingChars = 0;

  events.forEach((event) => {
    const payload = payloadAsRecord(event.payload);
    const chunkCountValue = payloadAsOptionalNumber(payload?.chunk_count) || 1;
    const chunkCharsValue =
      payloadAsOptionalNumber(payload?.chunk_chars) ||
      (payloadAsOptionalString(payload?.text) ||
        payloadAsOptionalString(payload?.chunk) ||
        '').length;
    if (event.event_type === 'chunk') {
      chunkCount += chunkCountValue;
      chunkChars += chunkCharsValue;
    }
    if (event.event_type === 'thinking') {
      thinkingCount += chunkCountValue;
      thinkingChars += chunkCharsValue;
    }
  });

  return {
    chunkCount,
    chunkChars,
    thinkingCount,
    thinkingChars,
  };
}

export function describeRunEventType(event: TaskRunEventRecord, t: TranslateFn): string {
  if (event.event_type === 'chunk') {
    return t('runs.event.modelReply');
  }
  if (event.event_type === 'thinking') {
    return t('runs.event.thinking');
  }
  if (event.event_type === 'plugin_runtime') {
    return t('runs.event.pluginRuntime');
  }
  return event.event_type;
}

export function RunEventPayload({
  event,
  t,
}: {
  event: TaskRunEventRecord;
  t: TranslateFn;
}) {
  const payload = payloadAsRecord(event.payload);
  const aggregatedText = payloadAsOptionalString(payload?.text);
  if (
    (event.event_type === 'chunk' || event.event_type === 'thinking') &&
    aggregatedText !== undefined
  ) {
    const aggregatedCount = payloadAsOptionalNumber(payload?.chunk_count) || 1;
    const aggregatedChars =
      payloadAsOptionalNumber(payload?.chunk_chars) || aggregatedText.length;
    return (
      <Space direction="vertical" size={8} style={{ width: '100%' }}>
        <Typography.Text type="secondary">
          {t('runs.event.fragmentSummary', {
            count: aggregatedCount,
            chars: aggregatedChars,
          })}
        </Typography.Text>
        <Typography.Paragraph
          style={{
            background: '#fafafa',
            padding: 12,
            borderRadius: 6,
            marginBottom: 0,
            whiteSpace: 'pre-wrap',
          }}
          ellipsis={{ rows: 8, expandable: 'collapsible' }}
        >
          {aggregatedText || '(empty)'}
        </Typography.Paragraph>
      </Space>
    );
  }

  if (!event.payload) {
    return null;
  }

  return <CollapsiblePayload value={event.payload} t={t} />;
}

function dedupeToolCalls(items: ToolCallView[]): ToolCallView[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = item.invocationId || `${item.callId}::${item.name}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function extractToolCallArray(value: unknown): ToolCallView[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => payloadAsRecord(item))
    .filter((item): item is Record<string, unknown> => Boolean(item))
    .map((toolCall) => ({
      invocationId: payloadAsOptionalString(toolCall.invocation_id) || '',
      callId:
        payloadAsOptionalString(toolCall.id) ||
        payloadAsOptionalString(toolCall.call_id) ||
        payloadAsOptionalString(toolCall.tool_call_id) ||
        '',
      name:
        payloadAsOptionalString(toolCall.name) ||
        payloadAsOptionalString(payloadAsRecord(toolCall.function)?.name) ||
        'unknown_tool',
      arguments:
        parseJsonLike(
          payloadAsOptionalString(toolCall.arguments) ||
            payloadAsOptionalString(payloadAsRecord(toolCall.function)?.arguments),
        ) ??
        toolCall.arguments ??
        payloadAsRecord(toolCall.function)?.arguments,
    }))
    .filter((item) => item.name);
}

function parseJsonLike(value: string | undefined): unknown {
  if (!value) {
    return undefined;
  }
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

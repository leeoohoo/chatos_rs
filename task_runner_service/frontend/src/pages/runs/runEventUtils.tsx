// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Space, Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteServerRecord, TaskRunEventRecord } from '../../types';
import {
  isRemoteToolName,
  payloadAsOptionalBoolean,
  payloadAsOptionalNumber,
  payloadAsOptionalString,
  payloadAsRecord,
  summarizeRemoteOperationStats,
} from '../shared/remoteOperationUtils';
import { CollapsiblePayload } from './payloadView';

export { formatRemoteEndpoint } from '../shared/remoteOperationUtils';

export type ToolCallView = {
  callId: string;
  name: string;
  arguments?: unknown;
};

export type ToolResultView = {
  toolCallId: string;
  name: string;
  success: boolean;
  content: string;
  result?: unknown;
};

export type RemoteOperationView = {
  toolCallId: string;
  name: string;
  success: boolean;
  connectionId?: string;
  connectionName?: string;
  username?: string;
  host?: string;
  port?: number;
  command?: string;
  path?: string;
  remoteHost?: string;
  output?: string;
  outputTruncated?: boolean;
  entryCount?: number;
  sourceSizeBytes?: number;
  outputChars?: number;
  maxBytes?: number;
  content?: string;
  result?: unknown;
  summary?: string;
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
  return events
    .filter((event) => event.event_type === 'tool_stream')
    .map((event) => payloadAsRecord(event.payload))
    .filter((payload): payload is Record<string, unknown> => Boolean(payload))
    .map((payload) => ({
      toolCallId: payloadAsOptionalString(payload.tool_call_id) || '',
      name: payloadAsOptionalString(payload.name) || 'unknown_tool',
      success: Boolean(payload.success) && !Boolean(payload.is_error),
      content: payloadAsOptionalString(payload.content) || '',
      result: payload.result,
    }));
}

export function collectRemoteToolOperations(
  toolCalls: ToolCallView[],
  toolResults: ToolResultView[],
  remoteServerMap: Map<string, RemoteServerRecord>,
): RemoteOperationView[] {
  const toolCallMap = new Map<string, ToolCallView>();
  toolCalls.forEach((toolCall) => {
    toolCallMap.set(`${toolCall.callId}::${toolCall.name}`, toolCall);
  });

  return toolResults
    .filter((result) => isRemoteToolName(result.name))
    .map((result) => {
      const toolCall = toolCallMap.get(`${result.toolCallId}::${result.name}`);
      const toolCallArgs = payloadAsRecord(toolCall?.arguments);
      const structured = payloadAsRecord(result.result);
      const nestedResult = payloadAsRecord(structured?.result);
      const connectionId =
        payloadAsOptionalString(structured?.connection_id) ||
        payloadAsOptionalString(toolCallArgs?.connection_id);
      const remoteServer = connectionId ? remoteServerMap.get(connectionId) : undefined;
      const name = payloadAsOptionalString(structured?.name) || remoteServer?.name || result.name;
      const username = payloadAsOptionalString(structured?.username) || remoteServer?.username;
      const host = payloadAsOptionalString(structured?.host) || remoteServer?.host;
      const port = payloadAsOptionalNumber(structured?.port) || remoteServer?.port;
      const command =
        payloadAsOptionalString(structured?.command) ||
        payloadAsOptionalString(toolCallArgs?.command);
      const path =
        payloadAsOptionalString(structured?.path) || payloadAsOptionalString(toolCallArgs?.path);
      const remoteHost = payloadAsOptionalString(nestedResult?.remote_host);
      const output = payloadAsOptionalString(structured?.output);
      const outputTruncated = payloadAsOptionalBoolean(structured?.output_truncated);
      const entryCount = payloadAsOptionalNumber(structured?.count);
      const sourceSizeBytes = payloadAsOptionalNumber(structured?.source_size_bytes);
      const outputChars = payloadAsOptionalNumber(structured?.output_chars);
      const maxBytes = payloadAsOptionalNumber(structured?.max_bytes);

      return {
        toolCallId: result.toolCallId,
        name: result.name,
        success: result.success,
        connectionId,
        connectionName: name,
        username,
        host,
        port,
        command,
        path,
        remoteHost,
        output,
        outputTruncated,
        entryCount,
        sourceSizeBytes,
        outputChars,
        maxBytes,
        content: result.content,
        result: result.result,
        summary: summarizeRemoteOperation(result.name, command, path, outputChars, entryCount),
      };
    });
}

export function summarizeRemoteOperations(items: RemoteOperationView[]) {
  return summarizeRemoteOperationStats(items);
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

export function formatRemoteVolume(operation: RemoteOperationView): string {
  if (operation.entryCount !== undefined) {
    return `${operation.entryCount} entries`;
  }
  if (operation.sourceSizeBytes !== undefined) {
    return `${operation.sourceSizeBytes} bytes`;
  }
  if (operation.outputChars !== undefined) {
    return `${operation.outputChars} chars`;
  }
  if (operation.maxBytes !== undefined) {
    return `limit ${operation.maxBytes} bytes`;
  }
  return '-';
}

function dedupeToolCalls(items: ToolCallView[]): ToolCallView[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = `${item.callId}::${item.name}`;
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

function summarizeRemoteOperation(
  name: string,
  command?: string,
  path?: string,
  outputChars?: number,
  entryCount?: number,
): string | undefined {
  if (name === 'run_command' && command) {
    return command;
  }
  if ((name === 'list_directory' || name === 'read_file') && path) {
    return path;
  }
  if (name === 'list_connections') {
    return entryCount === undefined ? undefined : `${entryCount} connections`;
  }
  if (name === 'run_command' && outputChars !== undefined) {
    return `${outputChars} chars`;
  }
  return undefined;
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

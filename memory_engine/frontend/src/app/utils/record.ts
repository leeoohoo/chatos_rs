// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineRecord } from '../../types';
import type { ToolSection } from '../types';

import { isObjectRecord, textOrUndefined } from './common';

export function formatStructuredText(value: unknown): string {
  if (value === null || value === undefined) {
    return '';
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) {
      return '';
    }
    try {
      return JSON.stringify(JSON.parse(trimmed), null, 2);
    } catch {
      return value;
    }
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function isJsonLikeText(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

export function getRecordToolSections(record: EngineRecord): ToolSection[] {
  const metadata = isObjectRecord(record.metadata) ? record.metadata : null;
  const sections: ToolSection[] = [];
  const rawToolCalls = metadata ? metadata.toolCalls ?? metadata.tool_calls : undefined;

  if (Array.isArray(rawToolCalls)) {
    rawToolCalls.forEach((toolCall, index) => {
      let toolName = `工具调用 ${index + 1}`;
      if (isObjectRecord(toolCall)) {
        const toolFunction = toolCall.function;
        if (
          isObjectRecord(toolFunction) &&
          typeof toolFunction.name === 'string' &&
          toolFunction.name.trim()
        ) {
          toolName = toolFunction.name;
        }
      }
      sections.push({
        key: `call-${index}`,
        label: `工具调用 ${index + 1} · ${toolName}`,
        body: formatStructuredText(toolCall),
      });
    });
  }

  const toolName =
    metadata && typeof metadata.toolName === 'string' && metadata.toolName.trim()
      ? metadata.toolName
      : undefined;
  const resultBody =
    metadata?.structured_result ?? record.structured_payload ?? textOrUndefined(record.content);
  if ((record.role === 'tool' || toolName) && resultBody !== undefined) {
    sections.push({
      key: 'result',
      label: toolName ? `工具结果 · ${toolName}` : '工具结果',
      body: formatStructuredText(resultBody),
    });
  }

  return sections.filter((section) => section.body.trim());
}

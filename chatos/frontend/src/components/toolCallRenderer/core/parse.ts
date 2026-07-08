// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../../types';
import { asRecord } from './value';
import { CODE_READ_TOOL_NAMES } from './toolName';

export type NormalizedToolResult = {
  value: unknown;
  parsed: Record<string, unknown> | null;
};

export const extractStructuredToolMessageResult = (message?: Message): unknown => {
  if (!message) return undefined;
  const metadata = message.metadata;
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    if (Object.prototype.hasOwnProperty.call(metadata, 'structured_result')) {
      return (metadata as Record<string, unknown>).structured_result;
    }
    if (Object.prototype.hasOwnProperty.call(metadata, 'structuredResult')) {
      return (metadata as Record<string, unknown>).structuredResult;
    }
  }
  return message.content;
};

const extractCodeFenceContents = (value: string): string[] => {
  const matches = value.matchAll(/```(?:[\w-]+)?\s*([\s\S]*?)```/g);
  const candidates: string[] = [];

  for (const match of matches) {
    const candidate = (match[1] || '').trim();
    if (candidate.length > 0) {
      candidates.push(candidate);
    }
  }

  return candidates;
};

const extractBalancedJsonObject = (value: string): string | null => {
  for (let start = value.indexOf('{'); start >= 0; start = value.indexOf('{', start + 1)) {
    let depth = 0;
    let inString = false;
    let escaped = false;

    for (let index = start; index < value.length; index += 1) {
      const char = value[index];

      if (inString) {
        if (escaped) {
          escaped = false;
        } else if (char === '\\') {
          escaped = true;
        } else if (char === '"') {
          inString = false;
        }
        continue;
      }

      if (char === '"') {
        inString = true;
        continue;
      }

      if (char === '{') {
        depth += 1;
        continue;
      }

      if (char === '}') {
        depth -= 1;
        if (depth === 0) {
          return value.slice(start, index + 1);
        }
      }
    }
  }

  return null;
};

const extractJsonishStringField = (
  value: string,
  keys: string[],
): string | undefined => {
  const keyPattern = keys
    .map((key) => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|');
  const match = value.match(
    new RegExp(`"(?:${keyPattern})"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`, 's'),
  );

  if (!match) {
    return undefined;
  }

  const normalized = match[1]
    .replace(/\r/g, '\\r')
    .replace(/\n/g, '\\n');

  try {
    const parsed = JSON.parse(`"${normalized}"`);
    return typeof parsed === 'string' ? parsed : undefined;
  } catch {
    return undefined;
  }
};

const extractJsonishNumberField = (
  value: string,
  keys: string[],
): number | undefined => {
  const keyPattern = keys
    .map((key) => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|');
  const match = value.match(
    new RegExp(`"(?:${keyPattern})"\\s*:\\s*(-?\\d+(?:\\.\\d+)?)`, 'i'),
  );

  if (!match) {
    return undefined;
  }

  const parsed = Number(match[1]);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const extractJsonishBooleanField = (
  value: string,
  keys: string[],
): boolean | undefined => {
  const keyPattern = keys
    .map((key) => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|');
  const match = value.match(
    new RegExp(`"(?:${keyPattern})"\\s*:\\s*(true|false)`, 'i'),
  );

  if (!match) {
    return undefined;
  }

  return match[1].toLowerCase() === 'true';
};

const parseMaybeStructuredValue = (value: unknown): Record<string, unknown> | null => {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }

  if (typeof value !== 'string') {
    return null;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  const candidates = [
    trimmed,
    ...extractCodeFenceContents(trimmed),
  ];
  const extractedObject = extractBalancedJsonObject(trimmed);
  if (extractedObject && !candidates.includes(extractedObject)) {
    candidates.push(extractedObject);
  }

  const visited = new Set<string>();

  for (const candidate of candidates) {
    const normalized = candidate.trim();
    if (!normalized || visited.has(normalized)) {
      continue;
    }
    visited.add(normalized);

    try {
      const parsed = JSON.parse(normalized);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed as Record<string, unknown>;
      }
      if (typeof parsed === 'string') {
        const nested = parseMaybeStructuredValue(parsed);
        if (nested) {
          return nested;
        }
      }
    } catch {
      continue;
    }
  }

  return null;
};

const extractCodeReadPayloadFromText = (value: string): Record<string, unknown> | null => {
  if (!/"(?:path|content|size_bytes|line_count|start_line|end_line|total_lines|ends_with_newline)"/.test(value)) {
    return null;
  }

  const content = extractJsonishStringField(value, ['content']);
  if (content === undefined) {
    return null;
  }

  const payload: Record<string, unknown> = {
    content,
  };

  const path = extractJsonishStringField(value, ['path']);
  const sha256 = extractJsonishStringField(value, ['sha256']);
  const sizeBytes = extractJsonishNumberField(value, ['size_bytes', 'sizeBytes']);
  const lineCount = extractJsonishNumberField(value, ['line_count', 'lineCount']);
  const startLine = extractJsonishNumberField(value, ['start_line', 'startLine']);
  const endLine = extractJsonishNumberField(value, ['end_line', 'endLine']);
  const totalLines = extractJsonishNumberField(value, ['total_lines', 'totalLines']);
  const endsWithNewline = extractJsonishBooleanField(value, ['ends_with_newline', 'endsWithNewline']);

  if (path !== undefined) payload.path = path;
  if (sha256 !== undefined) payload.sha256 = sha256;
  if (sizeBytes !== undefined) payload.size_bytes = sizeBytes;
  if (lineCount !== undefined) payload.line_count = lineCount;
  if (startLine !== undefined) payload.start_line = startLine;
  if (endLine !== undefined) payload.end_line = endLine;
  if (totalLines !== undefined) payload.total_lines = totalLines;
  if (endsWithNewline !== undefined) payload.ends_with_newline = endsWithNewline;

  return payload;
};

export const parseToolStructuredValue = (
  value: unknown,
  displayName: string,
): Record<string, unknown> | null => {
  const parsed = parseMaybeStructuredValue(value);
  if (parsed) {
    return parsed;
  }

  if (typeof value !== 'string') {
    return null;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  if (CODE_READ_TOOL_NAMES.has(displayName)) {
    return extractCodeReadPayloadFromText(trimmed);
  }

  return null;
};

export const normalizeToolResult = (
  candidates: unknown[],
  displayToolName: string,
): NormalizedToolResult => {
  for (const candidate of candidates) {
    if (candidate && typeof candidate === 'object') {
      return {
        value: candidate,
        parsed: asRecord(candidate),
      };
    }
  }

  for (const candidate of candidates) {
    const parsed = parseToolStructuredValue(candidate, displayToolName);
    if (parsed) {
      return {
        value: parsed,
        parsed,
      };
    }
  }

  for (const candidate of candidates) {
    if (typeof candidate === 'string' && candidate.trim().length > 0) {
      return {
        value: candidate,
        parsed: null,
      };
    }
    if (candidate !== undefined && candidate !== null) {
      return {
        value: candidate,
        parsed: null,
      };
    }
  }

  return {
    value: undefined,
    parsed: null,
  };
};

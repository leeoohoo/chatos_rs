// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const TEXT_PREVIEW_MAX_CHARS = 16_000;
const JSON_PREVIEW_MAX_CHARS = 20_000;
const JSON_STRING_MAX_CHARS = 512;
const JSON_ARRAY_MAX_ITEMS = 32;
const JSON_OBJECT_MAX_KEYS = 80;
const JSON_MAX_DEPTH = 6;

interface PreviewResult<T> {
  value: T;
  truncated: boolean;
}

export interface ToolDetailTextPreview {
  content: string;
  meta?: string;
}

const truncateString = (value: string, maxChars: number): PreviewResult<string> => {
  if (value.length <= maxChars) {
    return { value, truncated: false };
  }
  return {
    value: `${value.slice(0, Math.max(0, maxChars - 32))}\n... truncated ${value.length - maxChars} chars`,
    truncated: true,
  };
};

export const formatToolDetailText = (
  value: string,
  maxChars: number = TEXT_PREVIEW_MAX_CHARS,
): ToolDetailTextPreview => {
  const trimmed = value.trim();
  const preview = truncateString(trimmed, maxChars);
  return {
    content: preview.value,
    meta: preview.truncated ? 'truncated' : undefined,
  };
};

const previewJsonValue = (
  value: unknown,
  depth: number,
  seen: WeakSet<object>,
): PreviewResult<unknown> => {
  if (
    value === null
    || typeof value === 'number'
    || typeof value === 'boolean'
  ) {
    return { value, truncated: false };
  }

  if (typeof value === 'string') {
    return truncateString(value, JSON_STRING_MAX_CHARS);
  }

  if (typeof value !== 'object') {
    return { value: String(value), truncated: true };
  }

  if (seen.has(value)) {
    return { value: '[Circular]', truncated: true };
  }

  if (depth >= JSON_MAX_DEPTH) {
    return { value: '[Max depth reached]', truncated: true };
  }

  seen.add(value);

  if (Array.isArray(value)) {
    let truncated = value.length > JSON_ARRAY_MAX_ITEMS;
    const items = value
      .slice(0, JSON_ARRAY_MAX_ITEMS)
      .map((item) => {
        const preview = previewJsonValue(item, depth + 1, seen);
        truncated = truncated || preview.truncated;
        return preview.value;
      });
    if (value.length > JSON_ARRAY_MAX_ITEMS) {
      items.push(`... ${value.length - JSON_ARRAY_MAX_ITEMS} more items`);
    }
    seen.delete(value);
    return { value: items, truncated };
  }

  let truncated = false;
  let kept = 0;
  let omitted = 0;
  const record = value as Record<string, unknown>;
  const output: Record<string, unknown> = {};

  for (const key in record) {
    if (!Object.prototype.hasOwnProperty.call(record, key)) {
      continue;
    }
    if (kept >= JSON_OBJECT_MAX_KEYS) {
      omitted += 1;
      truncated = true;
      continue;
    }
    const preview = previewJsonValue(record[key], depth + 1, seen);
    output[key] = preview.value;
    kept += 1;
    truncated = truncated || preview.truncated;
  }

  if (omitted > 0) {
    output.__truncated__ = `... ${omitted} more keys`;
  }

  seen.delete(value);
  return { value: output, truncated };
};

export const stringifyJsonPreview = (
  value: unknown,
  maxChars: number = JSON_PREVIEW_MAX_CHARS,
): ToolDetailTextPreview => {
  const preview = previewJsonValue(value, 0, new WeakSet<object>());
  try {
    const serialized = JSON.stringify(preview.value, null, 2);
    const clipped = truncateString(serialized, maxChars);
    return {
      content: clipped.value,
      meta: preview.truncated || clipped.truncated ? 'truncated' : undefined,
    };
  } catch {
    return { content: '', meta: undefined };
  }
};

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export function parseJsonObject(value: unknown, fallback: Record<string, unknown> = {}) {
  if (typeof value !== 'string' || !value.trim()) {
    return fallback;
  }
  const parsed = JSON.parse(value) as unknown;
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error('JSON 必须是对象');
  }
  return parsed as Record<string, unknown>;
}

export function parseJsonArray(value: unknown, fallback: unknown[] = []) {
  if (typeof value !== 'string' || !value.trim()) {
    return fallback;
  }
  const parsed = JSON.parse(value) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error('JSON 必须是数组');
  }
  return parsed;
}

export function jsonText(value: unknown): string {
  if (value === undefined || value === null) {
    return '';
  }
  return JSON.stringify(value, null, 2);
}

export function optionalText(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed || undefined;
}

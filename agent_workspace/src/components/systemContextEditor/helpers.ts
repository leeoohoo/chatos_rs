import type { SystemContextLike } from './types';

export function splitLines(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

export function readContextContent(context: SystemContextLike): string {
  return typeof context.content === 'string' ? context.content : '';
}

export function readContextName(context: SystemContextLike): string {
  return typeof context.name === 'string' ? context.name : '';
}

export function readContextUpdatedAt(context: SystemContextLike): string {
  const raw = context.updatedAt || context.updated_at || context.createdAt || context.created_at;
  if (!raw) {
    return '-';
  }

  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) {
    return String(raw);
  }

  return date.toLocaleString();
}

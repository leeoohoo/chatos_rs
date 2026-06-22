import type { UiLocale } from '../../i18n/messages';
import { formatToolPrimitive } from '../../i18n/toolText';

export const asRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

export const asString = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

export const isPrimitive = (value: unknown): value is string | number | boolean | null => (
  value === null
  || typeof value === 'string'
  || typeof value === 'number'
  || typeof value === 'boolean'
);

export const formatLabel = (value: string): string => (
  value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')
    .trim()
    .toLowerCase()
);

const TITLE_CASE_OVERRIDES: Record<string, string> = {
  api: 'API',
  html: 'HTML',
  id: 'ID',
  js: 'JS',
  json: 'JSON',
  url: 'URL',
  urls: 'URLs',
};

export const formatCardTitle = (value: string): string => (
  formatLabel(value)
    .split(' ')
    .filter(Boolean)
    .map((part) => TITLE_CASE_OVERRIDES[part] || `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(' ')
);

export const formatPrimitive = (
  value: string | number | boolean | null,
  locale: UiLocale = 'zh-CN',
): string => formatToolPrimitive(value, locale);

export const isUrlLike = (value: string): boolean => /^https?:\/\//i.test(value.trim());

export const shouldRenderAsLongText = (key: string, value: string): boolean => (
  value.includes('\n')
  || value.length > 160
  || /(content|text|prompt|script|code|patch|diff|html|markdown|body|message|instruction|analysis|query)/i.test(key)
);

export const truncateText = (value: string, maxLength: number = 240): string => {
  const trimmed = value.trim();
  if (trimmed.length <= maxLength) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxLength - 1)}...`;
};

import { asArray } from './value';
import { isResearchToolName } from './toolName';

const OMITTED_STRUCTURED_RESULT_KEYS = new Set([
  '_summary_text',
  'summary_text',
  'summaryText',
  'research_findings',
  'researchFindings',
]);

const RESEARCH_STRUCTURED_RESULT_OMITTED_PATHS = new Set([
  'page.snapshot',
  'page.console_messages',
  'page.js_errors',
  'page.messages_brief',
  'page.errors_brief',
  'page.message_count_by_type',
  'search.data',
  'extract.results',
]);

const shouldOmitStructuredResultPath = (
  toolName: string,
  path: string,
  value: unknown,
): boolean => {
  const key = path.split('.').pop() ?? path;
  if (OMITTED_STRUCTURED_RESULT_KEYS.has(key)) {
    return true;
  }

  if (!isResearchToolName(toolName)) {
    return false;
  }

  if (RESEARCH_STRUCTURED_RESULT_OMITTED_PATHS.has(path)) {
    return true;
  }

  if (
    (path === 'search.provider_attempts' || path === 'extract.provider_attempts')
    && asArray(value).length === 0
  ) {
    return true;
  }

  return false;
};

export const sanitizeStructuredResultForDisplay = (
  value: unknown,
  toolName: string,
  path: string = '',
): unknown => {
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeStructuredResultForDisplay(item, toolName, path));
  }
  if (!value || typeof value !== 'object') {
    return value;
  }

  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([key, nestedValue]) => {
      const currentPath = path ? `${path}.${key}` : key;
      return !shouldOmitStructuredResultPath(toolName, currentPath, nestedValue);
    })
    .map(([key, nestedValue]) => {
      const currentPath = path ? `${path}.${key}` : key;
      return [key, sanitizeStructuredResultForDisplay(nestedValue, toolName, currentPath)];
    });

  return Object.fromEntries(entries);
};

export const hasStructuredContent = (value: unknown): boolean => {
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (!value || typeof value !== 'object') {
    return false;
  }
  return Object.keys(value as Record<string, unknown>).length > 0;
};

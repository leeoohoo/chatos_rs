import { LazyMarkdownRenderer } from '../../LazyMarkdownRenderer';
import type { ToolResultRenderer } from './types';

const stringifyResult = (value: unknown): string => {
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value);
  } catch {
    return '';
  }
};

export const fallbackToolResultRenderer: ToolResultRenderer = {
  id: 'fallback',
  sourceLabel: '通用面板',
  matches: () => true,
  render: ({ result }) => <LazyMarkdownRenderer content={stringifyResult(result)} />,
};

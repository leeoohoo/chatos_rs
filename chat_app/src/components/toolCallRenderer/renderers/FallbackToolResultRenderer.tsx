import { LazyMarkdownRenderer } from '../../LazyMarkdownRenderer';
import { stringifyJsonPreview } from '../../toolDetails/textPreview';
import { getToolRendererSourceLabel } from '../../../i18n/toolText';
import type { ToolResultRenderer } from './types';

const stringifyResult = (value: unknown): string => {
  if (typeof value === 'string') {
    return value;
  }
  return stringifyJsonPreview(value).content;
};

export const fallbackToolResultRenderer: ToolResultRenderer = {
  id: 'fallback',
  sourceLabel: (locale) => getToolRendererSourceLabel('structured', locale),
  matches: () => true,
  render: ({ result }) => <LazyMarkdownRenderer content={stringifyResult(result)} />,
};

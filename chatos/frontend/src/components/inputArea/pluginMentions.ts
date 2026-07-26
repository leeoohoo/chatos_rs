// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';

const PLUGIN_MENTION_QUERY_PATTERN = '[\\p{L}\\p{N}._-]';
const PLUGIN_MENTION_PREFIX = new RegExp(
  `(?:^|\\s)@(${PLUGIN_MENTION_QUERY_PATTERN}{0,128})$`,
  'u',
);
const PLUGIN_MENTION_SUFFIX = new RegExp(`^${PLUGIN_MENTION_QUERY_PATTERN}*`, 'u');

export interface PluginMentionDraft {
  start: number;
  end: number;
  query: string;
}

export interface ReplacedPluginMention {
  message: string;
  cursor: number;
}

export const findPluginMentionAtCursor = (
  message: string,
  cursor: number,
): PluginMentionDraft | null => {
  const safeCursor = Math.max(0, Math.min(message.length, cursor));
  const match = message.slice(0, safeCursor).match(PLUGIN_MENTION_PREFIX);
  if (!match) {
    return null;
  }
  const typedQuery = String(match[1] || '');
  const start = safeCursor - typedQuery.length - 1;
  const remainingToken = message.slice(safeCursor).match(PLUGIN_MENTION_SUFFIX)?.[0] || '';
  return {
    start,
    end: safeCursor + remainingToken.length,
    query: typedQuery.toLowerCase(),
  };
};

export const replacePluginMention = (
  message: string,
  draft: PluginMentionDraft,
  pluginKey: string,
): ReplacedPluginMention => {
  const normalizedPluginKey = pluginKey.trim();
  const before = message.slice(0, draft.start);
  const after = message.slice(draft.end);
  const mention = `@${normalizedPluginKey}`;
  const separator = after.length === 0 || !/^\s/u.test(after) ? ' ' : '';
  return {
    message: `${before}${mention}${separator}${after}`,
    cursor: before.length + mention.length + separator.length,
  };
};

export const filterPluginMentionOptions = (
  plugins: TaskRunnerSelectablePluginResponse[],
  query: string,
): TaskRunnerSelectablePluginResponse[] => {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return plugins;
  }
  return plugins.filter((plugin) => (
    [
      plugin.plugin_key,
      plugin.display_name,
      plugin.description,
      plugin.version,
      ...(Array.isArray(plugin.component_keys) ? plugin.component_keys : []),
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase()
      .includes(normalized)
  ));
};

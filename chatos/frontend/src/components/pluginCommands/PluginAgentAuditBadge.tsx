// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Bot } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';

interface PluginAgentAuditBadgeProps {
  selection: unknown;
  compact?: boolean;
  className?: string;
  onOpen?: () => void;
}

const normalizeSelection = (value: unknown): { plugin_id: string; agent_id: string } | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const record = value as { plugin_id?: unknown; agent_id?: unknown };
  const pluginId = typeof record.plugin_id === 'string' ? record.plugin_id.trim() : '';
  const agentId = typeof record.agent_id === 'string' ? record.agent_id.trim() : '';
  return pluginId && agentId ? { plugin_id: pluginId, agent_id: agentId } : null;
};

const PluginAgentAuditBadge: React.FC<PluginAgentAuditBadgeProps> = ({
  selection,
  compact = false,
  className,
  onOpen,
}) => {
  const { t } = useI18n();
  const normalized = normalizeSelection(selection);
  if (!normalized) {
    return null;
  }

  const classes = cn(
    'flex min-w-0 items-center gap-2 rounded-md border border-emerald-400/50 bg-emerald-500/5 text-xs',
    compact ? 'mt-1.5 px-2 py-1.5' : 'mt-0 px-2.5 py-2',
    onOpen && 'cursor-pointer hover:border-emerald-500 hover:bg-emerald-500/10',
    className,
  );
  const content = (
    <>
      <span className="inline-flex shrink-0 items-center gap-1 font-medium text-emerald-700 dark:text-emerald-300">
        <Bot className="h-3.5 w-3.5" />
        {t('pluginAgentAudit.label')}
      </span>
      <span className="min-w-0 truncate font-mono" title={`${normalized.plugin_id}/${normalized.agent_id}`}>
        {`${normalized.plugin_id}/@${normalized.agent_id}`}
      </span>
    </>
  );

  if (onOpen) {
    return (
      <button
        type="button"
        className={classes}
        onClick={(event) => {
          event.stopPropagation();
          onOpen();
        }}
      >
        {content}
      </button>
    );
  }
  return <div className={classes}>{content}</div>;
};

export default PluginAgentAuditBadge;

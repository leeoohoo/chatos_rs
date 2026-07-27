// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Hash, Puzzle } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import { normalizePluginCommandAuditEntries } from './pluginCommandAudit';

interface PluginCommandAuditListProps {
  entries: unknown;
  compact?: boolean;
  className?: string;
  onOpen?: () => void;
}

const PluginCommandAuditList: React.FC<PluginCommandAuditListProps> = ({
  entries,
  compact = false,
  className,
  onOpen,
}) => {
  const { t } = useI18n();
  const normalized = normalizePluginCommandAuditEntries(entries);
  if (normalized.length === 0) {
    return null;
  }

  const content = (
    <>
      <span className="inline-flex shrink-0 items-center gap-1 font-medium text-violet-700 dark:text-violet-300">
        <Puzzle className="h-3.5 w-3.5" />
        {t('pluginCommandAudit.count', { count: normalized.length })}
      </span>
      <span className="flex min-w-0 flex-wrap gap-1.5">
        {normalized.map((entry) => {
          const hashPreview = entry.arguments_sha256?.slice(0, 10) || null;
          return (
            <span
              key={`${entry.plugin_id}:${entry.command_id}`}
              className="inline-flex max-w-full items-center gap-1 rounded border border-violet-300/70 bg-background/80 px-1.5 py-0.5 text-[11px] text-foreground"
              title={`${entry.plugin_id}/${entry.command_id}`}
            >
              <span className="truncate">{`${entry.plugin_id}/${entry.command_id}`}</span>
              {entry.arguments_present ? (
                <span className="inline-flex shrink-0 items-center gap-0.5 text-muted-foreground">
                  <Hash className="h-3 w-3" />
                  {hashPreview || t('pluginCommandAudit.argumentsPresent')}
                </span>
              ) : null}
            </span>
          );
        })}
      </span>
    </>
  );

  const classes = cn(
    'flex min-w-0 flex-wrap items-center gap-2 rounded-md border border-violet-300/50 bg-violet-500/5 text-xs',
    compact ? 'mt-1.5 px-2 py-1.5' : 'mt-3 px-2.5 py-2',
    onOpen && 'cursor-pointer hover:border-violet-400 hover:bg-violet-500/10',
    className,
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

export default PluginCommandAuditList;

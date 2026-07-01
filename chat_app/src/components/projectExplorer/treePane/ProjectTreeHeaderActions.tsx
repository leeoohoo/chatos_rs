// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';

export const ProjectTreeHeaderActions: React.FC<{
  actionLoading: boolean;
  actionReloadPath: string | null;
  onCreateDirectoryAtRoot: () => void;
  onCreateFileAtRoot: () => void;
  onRefresh: () => void;
}> = ({
  actionLoading,
  actionReloadPath,
  onCreateDirectoryAtRoot,
  onCreateFileAtRoot,
  onRefresh,
}) => {
  const { t } = useI18n();

  return (
    <div className="flex flex-wrap gap-1">
      <button
        type="button"
        onClick={onCreateDirectoryAtRoot}
        disabled={actionLoading}
        className="rounded border border-blue-500/40 px-2 py-1 text-[11px] text-blue-700 hover:bg-blue-500/10 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {t('projectExplorer.tree.createDirectoryAtRoot')}
      </button>
      <button
        type="button"
        onClick={onCreateFileAtRoot}
        disabled={actionLoading}
        className="rounded border border-blue-500/40 px-2 py-1 text-[11px] text-blue-700 hover:bg-blue-500/10 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {t('projectExplorer.tree.createFileAtRoot')}
      </button>
      <button
        type="button"
        onClick={onRefresh}
        disabled={!actionReloadPath || actionLoading}
        className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
      >
        {t('projectExplorer.tree.refresh')}
      </button>
    </div>
  );
};

export const ProjectTreeHeaderMessages: React.FC<{
  actionMessage: string | null;
  actionError: string | null;
}> = ({
  actionMessage,
  actionError,
}) => (
  <>
    {actionMessage && (
      <div className="truncate text-[11px] text-emerald-600" title={actionMessage}>
        {actionMessage}
      </div>
    )}
    {actionError && (
      <div className="truncate text-[11px] text-destructive" title={actionError}>
        {actionError}
      </div>
    )}
  </>
);

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { LocalConnectorWorkspaceOption } from '../CreateResourceModals';

interface ExecutionTargetSectionProps {
  workspaces: LocalConnectorWorkspaceOption[];
  loading: boolean;
  error: string | null;
  workspaceId: string;
  onWorkspaceChange: (workspaceId: string) => void;
  onRefresh: () => void;
}

export const ExecutionTargetSection: FC<ExecutionTargetSectionProps> = ({
  workspaces,
  loading,
  error,
  workspaceId,
  onWorkspaceChange,
  onRefresh,
}) => {
  const { t } = useI18n();
  const currentWorkspaceAvailable = workspaces.some((workspace) => workspace.id === workspaceId);

  return (
    <section className="rounded-lg border border-border bg-muted/20 p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <label className="text-sm font-medium text-foreground">
            {t('remoteConnection.executionTarget')}
          </label>
          <p className="mt-1 text-xs text-muted-foreground">
            {t('remoteConnection.executionTargetHint')}
          </p>
        </div>
        <button
          type="button"
          onClick={onRefresh}
          disabled={loading}
          className="rounded border border-border px-3 py-1.5 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          {loading ? t('common.loading') : t('common.refresh')}
        </button>
      </div>
      <select
        value={workspaceId}
        onChange={(event) => onWorkspaceChange(event.target.value)}
        disabled={loading || workspaces.length === 0}
        className="mt-3 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:cursor-not-allowed disabled:opacity-60"
      >
        <option value="">{t('remoteConnection.executionTargetPlaceholder')}</option>
        {workspaceId && !currentWorkspaceAvailable ? (
          <option value={workspaceId}>{t('remoteConnection.executionTargetUnavailable')}</option>
        ) : null}
        {workspaces.map((workspace) => (
          <option key={`${workspace.deviceId}:${workspace.id}`} value={workspace.id}>
            {workspace.label}
          </option>
        ))}
      </select>
      {error ? <p className="mt-2 text-xs text-destructive">{error}</p> : null}
      {!loading && !error && workspaces.length === 0 ? (
        <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">
          {t('remoteConnection.executionTargetEmpty')}
        </p>
      ) : null}
    </section>
  );
};

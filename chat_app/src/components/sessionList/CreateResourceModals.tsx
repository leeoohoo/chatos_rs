// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { getUserVisiblePath, resolveUserVisiblePathInput } from '../../lib/domain/filesystem';
import ManagerFormDialog from '../ui/ManagerFormDialog';
import { deriveNameFromPath } from './helpers';

export type ResourceSourceMode = 'server' | 'local_connector';

export interface LocalConnectorWorkspaceOption {
  id: string;
  deviceId: string;
  label: string;
  alias: string;
  deviceLabel?: string | null;
  deviceStatus?: string | null;
  status?: string | null;
}

export interface LocalConnectorDirectoryEntryOption {
  name: string;
  path: string;
  isDir: boolean;
}

const normalizeLocalConnectorDirectoryPath = (value: string | null | undefined): string => {
  const normalized = (value || '').trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return normalized && normalized !== '.' ? normalized : '.';
};

const formatLocalConnectorDisplayPath = (alias: string, relativePath: string): string => {
  const normalized = normalizeLocalConnectorDirectoryPath(relativePath);
  return normalized === '.' ? alias : `${alias}/${normalized}`;
};

interface CreateResourceModalProps {
  isOpen: boolean;
  title: string;
  pathLabel: string;
  previewLabel: string;
  pathValue: string;
  error: string | null;
  fallbackName: string;
  sourceMode: ResourceSourceMode;
  localConnectorWorkspaces: LocalConnectorWorkspaceOption[];
  localConnectorLoading: boolean;
  localConnectorError: string | null;
  localConnectorDirectoryPath: string;
  localConnectorDirectoryParent: string | null;
  localConnectorDirectoryEntries: LocalConnectorDirectoryEntryOption[];
  localConnectorDirectoryLoading: boolean;
  localConnectorDirectoryError: string | null;
  selectedLocalDirectoryPath: string;
  selectedLocalWorkspaceId: string;
  terminalCommand?: string;
  terminalArgs?: string;
  terminalOutput?: string | null;
  terminalExecuting?: boolean;
  onClose: () => void;
  onSourceModeChange: (value: ResourceSourceMode) => void;
  onPathChange: (value: string) => void;
  onOpenPicker: () => void;
  onRefreshLocalConnector: () => void;
  onSelectedLocalWorkspaceChange: (value: string) => void;
  onBrowseLocalConnectorDirectory: (path: string) => void;
  onSelectLocalConnectorDirectory: (path: string) => void;
  onCreateLocalConnectorDirectory: (name: string) => void;
  onTerminalCommandChange?: (value: string) => void;
  onTerminalArgsChange?: (value: string) => void;
  onSubmit: () => void;
}

const CreateResourceModal: React.FC<CreateResourceModalProps> = ({
  isOpen,
  title,
  pathLabel,
  previewLabel,
  pathValue,
  error,
  fallbackName,
  sourceMode,
  localConnectorWorkspaces,
  localConnectorLoading,
  localConnectorError,
  localConnectorDirectoryPath,
  localConnectorDirectoryParent,
  localConnectorDirectoryEntries,
  localConnectorDirectoryLoading,
  localConnectorDirectoryError,
  selectedLocalDirectoryPath,
  selectedLocalWorkspaceId,
  terminalCommand,
  terminalArgs,
  terminalOutput,
  terminalExecuting,
  onClose,
  onSourceModeChange,
  onPathChange,
  onOpenPicker,
  onRefreshLocalConnector,
  onSelectedLocalWorkspaceChange,
  onBrowseLocalConnectorDirectory,
  onSelectLocalConnectorDirectory,
  onCreateLocalConnectorDirectory,
  onTerminalCommandChange,
  onTerminalArgsChange,
  onSubmit,
}) => {
  const { t } = useI18n();
  const [newLocalDirectoryName, setNewLocalDirectoryName] = React.useState('');
  const displayPathValue = getUserVisiblePath(pathValue);
  const displayName = deriveNameFromPath(displayPathValue, fallbackName);
  const selectedWorkspace = localConnectorWorkspaces.find((item) => item.id === selectedLocalWorkspaceId) || null;
  const isLocalConnectorMode = sourceMode === 'local_connector';
  const isTerminalCommandMode = Boolean(onTerminalCommandChange);
  const submitLabel = isTerminalCommandMode && isLocalConnectorMode
    ? t('sessionList.resource.executeCommand')
    : t('common.create');
  const currentLocalPath = normalizeLocalConnectorDirectoryPath(localConnectorDirectoryPath);
  const selectedLocalPath = normalizeLocalConnectorDirectoryPath(selectedLocalDirectoryPath);
  const localPreviewName = deriveNameFromPath(
    selectedLocalPath === '.' ? selectedWorkspace?.alias || '' : selectedLocalPath,
    fallbackName,
  );
  const localPreviewPath = selectedWorkspace
    ? formatLocalConnectorDisplayPath(selectedWorkspace.alias, selectedLocalPath)
    : '';

  return (
    <ManagerFormDialog
      open={isOpen}
      title={title}
      widthClassName="max-w-xl"
      onClose={onClose}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
        className="space-y-4"
      >
        <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
          <div className="inline-flex rounded-lg border border-border bg-background p-1">
            <button
              type="button"
              onClick={() => onSourceModeChange('server')}
              className={`rounded-md px-3 py-1.5 text-xs transition-colors ${sourceMode === 'server' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-accent'}`}
            >
              {t('sessionList.resource.sourceServer')}
            </button>
            <button
              type="button"
              onClick={() => {
                onSourceModeChange('local_connector');
                onRefreshLocalConnector();
              }}
              className={`rounded-md px-3 py-1.5 text-xs transition-colors ${sourceMode === 'local_connector' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-accent'}`}
            >
              {t('sessionList.resource.sourceLocalConnector')}
            </button>
          </div>

          {isLocalConnectorMode ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <select
                  value={selectedLocalWorkspaceId}
                  onChange={(event) => onSelectedLocalWorkspaceChange(event.target.value)}
                  className="min-w-0 flex-1 rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  disabled={localConnectorLoading || localConnectorWorkspaces.length === 0}
                  autoFocus
                >
                  {localConnectorWorkspaces.length === 0 ? (
                    <option value="">
                      {localConnectorLoading
                        ? t('sessionList.resource.localConnectorLoading')
                        : t('sessionList.resource.localConnectorEmpty')}
                    </option>
                  ) : localConnectorWorkspaces.map((workspace) => (
                    <option key={workspace.id} value={workspace.id}>
                      {workspace.label}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={onRefreshLocalConnector}
                  className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
                >
                  {t('common.refresh')}
                </button>
              </div>
              {selectedWorkspace ? (
                <div className="text-xs text-muted-foreground">
                  {previewLabel}
                  <span className="text-foreground">{localPreviewName}</span>
                  {selectedWorkspace.deviceLabel ? (
                    <span className="ml-2 text-muted-foreground">{selectedWorkspace.deviceLabel}</span>
                  ) : null}
                  {localPreviewPath ? (
                    <span className="ml-2 text-muted-foreground">{localPreviewPath}</span>
                  ) : null}
                </div>
              ) : null}
              {!isTerminalCommandMode ? (
                <div className="space-y-2 rounded-lg border border-border bg-background p-3">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span>{t('sessionList.resource.localConnectorCurrentDirectory')}</span>
                    <span className="font-mono text-foreground">
                      {selectedWorkspace ? formatLocalConnectorDisplayPath(selectedWorkspace.alias, currentLocalPath) : '-'}
                    </span>
                    <button
                      type="button"
                      onClick={() => localConnectorDirectoryParent && onBrowseLocalConnectorDirectory(localConnectorDirectoryParent)}
                      disabled={!localConnectorDirectoryParent || localConnectorDirectoryLoading}
                      className="ml-auto rounded bg-muted px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent disabled:opacity-50"
                    >
                      {t('sessionList.resource.localConnectorParentDirectory')}
                    </button>
                    <button
                      type="button"
                      onClick={() => onSelectLocalConnectorDirectory(currentLocalPath === '.' ? '' : currentLocalPath)}
                      disabled={!selectedWorkspace || localConnectorDirectoryLoading}
                      className="rounded bg-muted px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent disabled:opacity-50"
                    >
                      {t('sessionList.resource.localConnectorSelectCurrent')}
                    </button>
                  </div>
                  <div className="max-h-40 overflow-auto rounded border border-border">
                    {localConnectorDirectoryLoading ? (
                      <div className="p-3 text-xs text-muted-foreground">
                        {t('sessionList.resource.localConnectorDirectoryLoading')}
                      </div>
                    ) : localConnectorDirectoryEntries.length === 0 ? (
                      <div className="p-3 text-xs text-muted-foreground">
                        {t('sessionList.resource.localConnectorDirectoryEmpty')}
                      </div>
                    ) : (
                      localConnectorDirectoryEntries.map((entry) => (
                        <button
                          key={entry.path}
                          type="button"
                          onClick={() => onBrowseLocalConnectorDirectory(entry.path)}
                          className={`flex w-full items-center justify-between border-b border-border px-3 py-2 text-left text-sm last:border-b-0 hover:bg-accent ${
                            normalizeLocalConnectorDirectoryPath(entry.path) === selectedLocalPath
                              ? 'bg-accent text-accent-foreground'
                              : ''
                          }`}
                        >
                          <span className="truncate">{entry.name}</span>
                          <span className="text-xs text-muted-foreground">
                            {t('sessionList.resource.localConnectorOpenDirectory')}
                          </span>
                        </button>
                      ))
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    <input
                      value={newLocalDirectoryName}
                      onChange={(event) => setNewLocalDirectoryName(event.target.value)}
                      className="min-w-0 flex-1 rounded border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                      placeholder={t('sessionList.resource.localConnectorNewDirectoryPlaceholder')}
                    />
                    <button
                      type="button"
                      onClick={() => {
                        const name = newLocalDirectoryName.trim();
                        if (!name) return;
                        onCreateLocalConnectorDirectory(name);
                        setNewLocalDirectoryName('');
                      }}
                      disabled={!selectedWorkspace || localConnectorDirectoryLoading || !newLocalDirectoryName.trim()}
                      className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent disabled:opacity-50"
                    >
                      {t('sessionList.resource.localConnectorCreateDirectory')}
                    </button>
                  </div>
                  {localConnectorDirectoryError ? (
                    <div className="text-xs text-destructive">{localConnectorDirectoryError}</div>
                  ) : null}
                </div>
              ) : null}
              {isTerminalCommandMode ? (
                <div className="space-y-3">
                  <div className="grid gap-3 sm:grid-cols-[1fr_1.2fr]">
                    <label className="text-sm text-muted-foreground">
                      {t('sessionList.resource.command')}
                      <input
                        value={terminalCommand || ''}
                        onChange={(event) => onTerminalCommandChange?.(event.target.value)}
                        className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="pwd"
                      />
                    </label>
                    <label className="text-sm text-muted-foreground">
                      {t('sessionList.resource.commandArgs')}
                      <input
                        value={terminalArgs || ''}
                        onChange={(event) => onTerminalArgsChange?.(event.target.value)}
                        className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="--version"
                      />
                    </label>
                  </div>
                  {terminalOutput ? (
                    <pre className="max-h-48 overflow-auto rounded border border-border bg-background p-3 text-xs text-foreground">
                      {terminalOutput}
                    </pre>
                  ) : null}
                </div>
              ) : null}
              {localConnectorError ? (
                <div className="text-xs text-destructive">{localConnectorError}</div>
              ) : null}
            </div>
          ) : (
            <div>
              <label className="text-sm text-muted-foreground">{pathLabel}</label>
              <div className="mt-1 flex items-center gap-2">
                <input
                  value={displayPathValue}
                  onChange={(event) => onPathChange(resolveUserVisiblePathInput(event.target.value, pathValue))}
                  className="flex-1 rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder={t('sessionList.resource.pathPlaceholder')}
                  autoFocus
                />
                <button
                  type="button"
                  onClick={onOpenPicker}
                  className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
                >
                  {t('sessionList.resource.chooseDirectory')}
                </button>
              </div>
            </div>
          )}

          {!isLocalConnectorMode && pathValue.trim() ? (
            <div className="text-xs text-muted-foreground">
              {previewLabel}
              <span className="text-foreground">{displayName}</span>
              <span className="ml-2 text-muted-foreground">{displayPathValue}</span>
            </div>
          ) : null}
          {error ? (
            <div className="text-xs text-destructive">{error}</div>
          ) : null}
        </div>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
          >
            {t('common.cancel')}
          </button>
          <button
            type="submit"
            disabled={Boolean(terminalExecuting)}
            className="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-60"
          >
            {terminalExecuting ? t('sessionList.resource.executing') : submitLabel}
          </button>
        </div>
      </form>
    </ManagerFormDialog>
  );
};

interface CreateProjectModalProps {
  isOpen: boolean;
  projectRoot: string;
  projectError: string | null;
  sourceMode?: ResourceSourceMode;
  localConnectorWorkspaces?: LocalConnectorWorkspaceOption[];
  localConnectorLoading?: boolean;
  localConnectorError?: string | null;
  localConnectorDirectoryPath?: string;
  localConnectorDirectoryParent?: string | null;
  localConnectorDirectoryEntries?: LocalConnectorDirectoryEntryOption[];
  localConnectorDirectoryLoading?: boolean;
  localConnectorDirectoryError?: string | null;
  selectedLocalDirectoryPath?: string;
  selectedLocalWorkspaceId?: string;
  onClose: () => void;
  onSourceModeChange?: (value: ResourceSourceMode) => void;
  onProjectRootChange: (value: string) => void;
  onOpenPicker: () => void;
  onRefreshLocalConnector?: () => void;
  onSelectedLocalWorkspaceChange?: (value: string) => void;
  onBrowseLocalConnectorDirectory?: (path: string) => void;
  onSelectLocalConnectorDirectory?: (path: string) => void;
  onCreateLocalConnectorDirectory?: (name: string) => void;
  onCreate: () => void;
}

export const CreateProjectModal: React.FC<CreateProjectModalProps> = ({
  isOpen,
  projectRoot,
  projectError,
  sourceMode = 'server',
  localConnectorWorkspaces = [],
  localConnectorLoading = false,
  localConnectorError = null,
  localConnectorDirectoryPath = '.',
  localConnectorDirectoryParent = null,
  localConnectorDirectoryEntries = [],
  localConnectorDirectoryLoading = false,
  localConnectorDirectoryError = null,
  selectedLocalDirectoryPath = '',
  selectedLocalWorkspaceId = '',
  onClose,
  onSourceModeChange = () => {},
  onProjectRootChange,
  onOpenPicker,
  onRefreshLocalConnector = () => {},
  onSelectedLocalWorkspaceChange = () => {},
  onBrowseLocalConnectorDirectory = () => {},
  onSelectLocalConnectorDirectory = () => {},
  onCreateLocalConnectorDirectory = () => {},
  onCreate,
}) => {
  const { t } = useI18n();

  return (
    <CreateResourceModal
      isOpen={isOpen}
      title={t('sessionList.resource.projectTitle')}
      pathLabel={t('sessionList.resource.projectDirectory')}
      previewLabel={t('sessionList.resource.projectDefaultName')}
      pathValue={projectRoot}
      error={projectError}
      fallbackName="Project"
      sourceMode={sourceMode}
      localConnectorWorkspaces={localConnectorWorkspaces}
      localConnectorLoading={localConnectorLoading}
      localConnectorError={localConnectorError}
      localConnectorDirectoryPath={localConnectorDirectoryPath}
      localConnectorDirectoryParent={localConnectorDirectoryParent}
      localConnectorDirectoryEntries={localConnectorDirectoryEntries}
      localConnectorDirectoryLoading={localConnectorDirectoryLoading}
      localConnectorDirectoryError={localConnectorDirectoryError}
      selectedLocalDirectoryPath={selectedLocalDirectoryPath}
      selectedLocalWorkspaceId={selectedLocalWorkspaceId}
      onClose={onClose}
      onSourceModeChange={onSourceModeChange}
      onPathChange={onProjectRootChange}
      onOpenPicker={onOpenPicker}
      onRefreshLocalConnector={onRefreshLocalConnector}
      onSelectedLocalWorkspaceChange={onSelectedLocalWorkspaceChange}
      onBrowseLocalConnectorDirectory={onBrowseLocalConnectorDirectory}
      onSelectLocalConnectorDirectory={onSelectLocalConnectorDirectory}
      onCreateLocalConnectorDirectory={onCreateLocalConnectorDirectory}
      onSubmit={onCreate}
    />
  );
};

interface CreateTerminalModalProps {
  isOpen: boolean;
  terminalRoot: string;
  terminalError: string | null;
  sourceMode?: ResourceSourceMode;
  localConnectorWorkspaces?: LocalConnectorWorkspaceOption[];
  localConnectorLoading?: boolean;
  localConnectorError?: string | null;
  localConnectorDirectoryPath?: string;
  localConnectorDirectoryParent?: string | null;
  localConnectorDirectoryEntries?: LocalConnectorDirectoryEntryOption[];
  localConnectorDirectoryLoading?: boolean;
  localConnectorDirectoryError?: string | null;
  selectedLocalDirectoryPath?: string;
  selectedLocalWorkspaceId?: string;
  terminalCommand?: string;
  terminalArgs?: string;
  terminalOutput?: string | null;
  terminalExecuting?: boolean;
  onClose: () => void;
  onSourceModeChange?: (value: ResourceSourceMode) => void;
  onTerminalRootChange: (value: string) => void;
  onOpenPicker: () => void;
  onRefreshLocalConnector?: () => void;
  onSelectedLocalWorkspaceChange?: (value: string) => void;
  onBrowseLocalConnectorDirectory?: (path: string) => void;
  onSelectLocalConnectorDirectory?: (path: string) => void;
  onCreateLocalConnectorDirectory?: (name: string) => void;
  onTerminalCommandChange?: (value: string) => void;
  onTerminalArgsChange?: (value: string) => void;
  onCreate: () => void;
}

export const CreateTerminalModal: React.FC<CreateTerminalModalProps> = ({
  isOpen,
  terminalRoot,
  terminalError,
  sourceMode = 'server',
  localConnectorWorkspaces = [],
  localConnectorLoading = false,
  localConnectorError = null,
  localConnectorDirectoryPath = '.',
  localConnectorDirectoryParent = null,
  localConnectorDirectoryEntries = [],
  localConnectorDirectoryLoading = false,
  localConnectorDirectoryError = null,
  selectedLocalDirectoryPath = '',
  selectedLocalWorkspaceId = '',
  terminalOutput = null,
  terminalExecuting = false,
  onClose,
  onSourceModeChange = () => {},
  onTerminalRootChange,
  onOpenPicker,
  onRefreshLocalConnector = () => {},
  onSelectedLocalWorkspaceChange = () => {},
  onBrowseLocalConnectorDirectory = () => {},
  onSelectLocalConnectorDirectory = () => {},
  onCreateLocalConnectorDirectory = () => {},
  onCreate,
}) => {
  const { t } = useI18n();

  return (
    <CreateResourceModal
      isOpen={isOpen}
      title={t('sessionList.resource.terminalTitle')}
      pathLabel={t('sessionList.resource.terminalDirectory')}
      previewLabel={t('sessionList.resource.terminalDefaultName')}
      pathValue={terminalRoot}
      error={terminalError}
      fallbackName="Terminal"
      sourceMode={sourceMode}
      localConnectorWorkspaces={localConnectorWorkspaces}
      localConnectorLoading={localConnectorLoading}
      localConnectorError={localConnectorError}
      localConnectorDirectoryPath={localConnectorDirectoryPath}
      localConnectorDirectoryParent={localConnectorDirectoryParent}
      localConnectorDirectoryEntries={localConnectorDirectoryEntries}
      localConnectorDirectoryLoading={localConnectorDirectoryLoading}
      localConnectorDirectoryError={localConnectorDirectoryError}
      selectedLocalDirectoryPath={selectedLocalDirectoryPath}
      selectedLocalWorkspaceId={selectedLocalWorkspaceId}
      terminalOutput={terminalOutput}
      terminalExecuting={terminalExecuting}
      onClose={onClose}
      onSourceModeChange={onSourceModeChange}
      onPathChange={onTerminalRootChange}
      onOpenPicker={onOpenPicker}
      onRefreshLocalConnector={onRefreshLocalConnector}
      onSelectedLocalWorkspaceChange={onSelectedLocalWorkspaceChange}
      onBrowseLocalConnectorDirectory={onBrowseLocalConnectorDirectory}
      onSelectLocalConnectorDirectory={onSelectLocalConnectorDirectory}
      onCreateLocalConnectorDirectory={onCreateLocalConnectorDirectory}
      onSubmit={onCreate}
    />
  );
};

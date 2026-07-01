// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import ManagerFormDialog from '../ui/ManagerFormDialog';
import { deriveNameFromPath } from './helpers';

interface CreateResourceModalProps {
  isOpen: boolean;
  title: string;
  pathLabel: string;
  previewLabel: string;
  pathValue: string;
  error: string | null;
  fallbackName: string;
  onClose: () => void;
  onPathChange: (value: string) => void;
  onOpenPicker: () => void;
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
  onClose,
  onPathChange,
  onOpenPicker,
  onSubmit,
}) => {
  const { t } = useI18n();

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
          <div>
            <label className="text-sm text-muted-foreground">{pathLabel}</label>
            <div className="mt-1 flex items-center gap-2">
              <input
                value={pathValue}
                onChange={(e) => onPathChange(e.target.value)}
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
          {pathValue.trim() ? (
            <div className="text-xs text-muted-foreground">
              {previewLabel}
              <span className="text-foreground">{deriveNameFromPath(pathValue, fallbackName)}</span>
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
            className="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90"
          >
            {t('common.create')}
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
  onClose: () => void;
  onProjectRootChange: (value: string) => void;
  onOpenPicker: () => void;
  onCreate: () => void;
}

export const CreateProjectModal: React.FC<CreateProjectModalProps> = ({
  isOpen,
  projectRoot,
  projectError,
  onClose,
  onProjectRootChange,
  onOpenPicker,
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
      onClose={onClose}
      onPathChange={onProjectRootChange}
      onOpenPicker={onOpenPicker}
      onSubmit={onCreate}
    />
  );
};

interface CreateTerminalModalProps {
  isOpen: boolean;
  terminalRoot: string;
  terminalError: string | null;
  onClose: () => void;
  onTerminalRootChange: (value: string) => void;
  onOpenPicker: () => void;
  onCreate: () => void;
}

export const CreateTerminalModal: React.FC<CreateTerminalModalProps> = ({
  isOpen,
  terminalRoot,
  terminalError,
  onClose,
  onTerminalRootChange,
  onOpenPicker,
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
      onClose={onClose}
      onPathChange={onTerminalRootChange}
      onOpenPicker={onOpenPicker}
      onSubmit={onCreate}
    />
  );
};

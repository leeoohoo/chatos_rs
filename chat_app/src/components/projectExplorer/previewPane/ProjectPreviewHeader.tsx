import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { formatFileSize } from '../../../lib/utils';
import type { FsReadResult } from '../../../types';

interface ProjectPreviewHeaderProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  isEditing?: boolean;
  canEdit?: boolean;
  hasUnsavedChanges?: boolean;
  savingFile?: boolean;
  onStartEditing?: () => void;
  onCancelEditing?: () => void;
  onSaveEditing?: () => void;
}

export const ProjectPreviewHeader: React.FC<ProjectPreviewHeaderProps> = ({
  selectedFile,
  selectedPath,
  isEditing = false,
  canEdit = false,
  hasUnsavedChanges = false,
  savingFile = false,
  onStartEditing,
  onCancelEditing,
  onSaveEditing,
}) => {
  const { t } = useI18n();
  return (
    <div className="border-b border-border bg-card px-4 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">
            {selectedFile?.name || (selectedPath ? t('projectExplorer.preview.header.unavailable') : t('projectExplorer.preview.header.default'))}
          </div>
          <div className="mt-1 flex items-center justify-between gap-4">
            <div className="min-w-0 truncate text-[11px] text-muted-foreground">
              {selectedFile?.path || selectedPath || t('projectExplorer.preview.header.selectFile')}
            </div>
            {selectedFile && (
              <div className="whitespace-nowrap text-[11px] text-muted-foreground">
                {formatFileSize(selectedFile.size)}
              </div>
            )}
          </div>
        </div>
        {canEdit && (
          <div className="flex shrink-0 items-center gap-2">
            {isEditing ? (
              <>
                <button
                  type="button"
                  onClick={onCancelEditing}
                  disabled={savingFile}
                  className="rounded border border-border px-2.5 py-1 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {t('common.cancel')}
                </button>
                <button
                  type="button"
                  onClick={onSaveEditing}
                  disabled={savingFile || !hasUnsavedChanges}
                  className="rounded bg-primary px-2.5 py-1 text-xs text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {savingFile ? t('common.saving') : t('common.save')}
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={onStartEditing}
                className="rounded border border-border px-2.5 py-1 text-xs hover:bg-accent"
              >
                {t('common.edit')}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default ProjectPreviewHeader;

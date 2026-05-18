import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { formatFileSize } from '../../../lib/utils';
import type { FsReadResult } from '../../../types';

interface ProjectPreviewHeaderProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
}

export const ProjectPreviewHeader: React.FC<ProjectPreviewHeaderProps> = ({
  selectedFile,
  selectedPath,
}) => {
  const { t } = useI18n();
  return (
    <div className="border-b border-border bg-card px-4 py-3">
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
  );
};

export default ProjectPreviewHeader;

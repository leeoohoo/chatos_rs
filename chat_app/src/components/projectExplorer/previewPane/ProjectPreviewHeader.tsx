import React from 'react';

import { formatFileSize } from '../../../lib/utils';
import type { FsReadResult } from '../../../types';

interface ProjectPreviewHeaderProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
}

export const ProjectPreviewHeader: React.FC<ProjectPreviewHeaderProps> = ({
  selectedFile,
  selectedPath,
}) => (
  <div className="border-b border-border bg-card px-4 py-3">
    <div className="truncate text-sm font-medium text-foreground">
      {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
    </div>
    <div className="mt-1 flex items-center justify-between gap-4">
      <div className="min-w-0 truncate text-[11px] text-muted-foreground">
        {selectedFile?.path || selectedPath || '请选择文件'}
      </div>
      {selectedFile && (
        <div className="whitespace-nowrap text-[11px] text-muted-foreground">
          {formatFileSize(selectedFile.size)}
        </div>
      )}
    </div>
  </div>
);

export default ProjectPreviewHeader;

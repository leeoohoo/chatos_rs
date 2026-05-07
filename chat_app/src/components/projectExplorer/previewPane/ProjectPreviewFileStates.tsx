import React from 'react';

import type { FsReadResult } from '../../../types';

export const ProjectPreviewLoadingState: React.FC = () => (
  <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>
);

export const ProjectPreviewDeletedPathState: React.FC = () => (
  <div className="p-4 text-sm text-muted-foreground">
    该路径已删除或不存在，当前仅支持查看变更记录。
  </div>
);

export const ProjectPreviewEmptyState: React.FC = () => (
  <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>
);

export const ProjectPreviewImageContent: React.FC<{
  selectedFile: FsReadResult;
}> = ({ selectedFile }) => {
  const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
  return (
    <div className="h-full overflow-auto p-4">
      <img
        src={src}
        alt={selectedFile.name}
        className="max-h-full max-w-full rounded border border-border"
      />
    </div>
  );
};

export const ProjectPreviewBinaryDownload: React.FC<{
  selectedFile: FsReadResult;
}> = ({ selectedFile }) => {
  const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
  return (
    <div className="space-y-2 p-4 text-sm text-muted-foreground">
      <div>该文件为二进制内容，暂不支持直接预览。</div>
      <a
        href={downloadHref}
        download={selectedFile.name || 'file'}
        className="inline-flex items-center rounded bg-primary px-3 py-1.5 text-primary-foreground transition-colors hover:bg-primary/90"
      >
        下载文件
      </a>
    </div>
  );
};

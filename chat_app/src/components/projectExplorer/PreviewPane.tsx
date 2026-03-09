import React, { useMemo } from 'react';
import hljs from 'highlight.js';

import type { ChangeLogItem, FsEntry, FsReadResult } from '../../types';
import { formatFileSize } from '../../lib/utils';
import { DiffPanel } from './ChangeLogPanels';
import { escapeHtml, getHighlightLanguage } from './utils';

interface ProjectPreviewPaneProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  error: string | null;
  selectedLog: ChangeLogItem | null;
}

export const ProjectPreviewPane: React.FC<ProjectPreviewPaneProps> = ({
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  error,
  selectedLog,
}) => {
  const preview = useMemo(() => {
    if (loadingFile) {
      return <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>;
    }
    if (!selectedFile) {
      if (selectedPath && !selectedEntry) {
        return (
          <div className="p-4 text-sm text-muted-foreground">
            该路径已删除或不存在，当前仅支持查看变更记录。
          </div>
        );
      }
      return <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>;
    }
    const isImage = selectedFile.contentType.startsWith('image/');
    if (isImage && selectedFile.isBinary) {
      const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
      return (
        <div className="p-4 overflow-auto h-full">
          <img src={src} alt={selectedFile.name} className="max-w-full max-h-full rounded border border-border" />
        </div>
      );
    }
    if (!selectedFile.isBinary) {
      const language = getHighlightLanguage(selectedFile.name);
      let highlighted = '';
      try {
        if (language) {
          highlighted = hljs.highlight(selectedFile.content, { language }).value;
        } else {
          highlighted = hljs.highlightAuto(selectedFile.content).value;
        }
      } catch {
        highlighted = escapeHtml(selectedFile.content);
      }
      const lines = highlighted.split(/\r?\n/);
      return (
        <div className="h-full overflow-auto bg-muted/30">
          <div className="flex min-h-full text-sm">
            <div className="shrink-0 py-4 pr-3 pl-2 border-r border-border text-right text-muted-foreground select-none">
              {lines.map((_, idx) => (
                <div key={idx} className="leading-5">
                  {idx + 1}
                </div>
              ))}
            </div>
            <div className="flex-1 min-w-0 py-4 pl-3 pr-4 hljs">
              {lines.map((line, idx) => (
                <div
                  key={idx}
                  className="leading-5 font-mono whitespace-pre w-full"
                  dangerouslySetInnerHTML={{ __html: line || '&nbsp;' }}
                />
              ))}
            </div>
          </div>
        </div>
      );
    }
    const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
    return (
      <div className="p-4 text-sm text-muted-foreground space-y-2">
        <div>该文件为二进制内容，暂不支持直接预览。</div>
        <a
          href={downloadHref}
          download={selectedFile.name || 'file'}
          className="inline-flex items-center px-3 py-1.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          下载文件
        </a>
      </div>
    );
  }, [loadingFile, selectedEntry, selectedFile, selectedPath]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-4 py-2 border-b border-border bg-card flex items-center justify-between">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground truncate">
            {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            {selectedFile?.path || selectedPath || '请选择文件'}
          </div>
        </div>
        {selectedFile && (
          <div className="text-[11px] text-muted-foreground">
            {formatFileSize(selectedFile.size)}
          </div>
        )}
      </div>
      <div className="flex-1 overflow-hidden flex flex-col">
        <DiffPanel selectedLog={selectedLog} />
        <div className="flex-1 min-h-0 overflow-hidden">
          {error ? (
            <div className="p-4 text-sm text-destructive">{error}</div>
          ) : (
            preview
          )}
        </div>
      </div>
    </div>
  );
};

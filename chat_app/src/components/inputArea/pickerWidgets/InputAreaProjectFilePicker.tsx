import React from 'react';

import { cn } from '../../../lib/utils';
import type { FsEntry } from '../../../types';

interface InputAreaProjectFilePickerProps {
  allowAttachments: boolean;
  showProjectFilePicker: boolean;
  pickerRef: React.RefObject<HTMLDivElement>;
  disabled: boolean;
  projectFileAttachingPath: string | null;
  projectFilePickerOpen: boolean;
  onTogglePicker: () => void;
  projectName: string;
  projectFilePathLabel: string;
  projectFileFilter: string;
  onProjectFileFilterChange: (value: string) => void;
  projectFileBusy: boolean;
  projectFileKeywordActive: boolean;
  projectFileParent: string | null;
  onLoadProjectFileEntries: (path?: string | null) => void;
  displayedProjectFileEntries: FsEntry[];
  onAttachProjectFile: (entry: FsEntry) => void;
  toRelativeProjectPath: (path: string) => string;
  projectFileSearchTruncated: boolean;
}

export const InputAreaProjectFilePicker: React.FC<InputAreaProjectFilePickerProps> = ({
  allowAttachments,
  showProjectFilePicker,
  pickerRef,
  disabled,
  projectFileAttachingPath,
  projectFilePickerOpen,
  onTogglePicker,
  projectName,
  projectFilePathLabel,
  projectFileFilter,
  onProjectFileFilterChange,
  projectFileBusy,
  projectFileKeywordActive,
  projectFileParent,
  onLoadProjectFileEntries,
  displayedProjectFileEntries,
  onAttachProjectFile,
  toRelativeProjectPath,
  projectFileSearchTruncated,
}) => {
  if (!allowAttachments || !showProjectFilePicker) {
    return null;
  }

  return (
    <div className="relative flex-shrink-0" ref={pickerRef}>
      <button
        type="button"
        onClick={onTogglePicker}
        disabled={disabled || projectFileAttachingPath !== null}
        className={cn(
          'px-2 py-1 rounded-md border text-xs transition-colors',
          'text-muted-foreground hover:text-foreground hover:bg-accent',
          (disabled || projectFileAttachingPath !== null) && 'opacity-50 cursor-not-allowed',
        )}
        title="从当前项目选择文件"
      >
        项目文件
        <span className="ml-1">▾</span>
      </button>
      {projectFilePickerOpen && (
        <div className="absolute left-0 bottom-full mb-2 z-30 w-80 bg-popover text-popover-foreground border rounded-md shadow-lg">
          <div className="px-3 py-2 border-b space-y-2">
            <div className="space-y-1">
              <div className="text-[11px] text-muted-foreground truncate" title={projectName || '当前项目'}>
                项目: {projectName || '当前项目'}
              </div>
              <div className="text-[11px] text-muted-foreground truncate font-mono" title={projectFilePathLabel || '/'}>
                路径: {projectFilePathLabel || '/'}
              </div>
            </div>
            <input
              type="text"
              value={projectFileFilter}
              onChange={(event) => onProjectFileFilterChange(event.target.value)}
              placeholder="筛选文件（不区分大小写，支持模糊）..."
              className="w-full rounded border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
            />
          </div>
          <div className="max-h-64 overflow-auto py-1">
            {projectFileBusy ? (
              <div className="px-3 py-2 text-xs text-muted-foreground">
                {projectFileKeywordActive ? '搜索中...' : '加载中...'}
              </div>
            ) : (
              <>
                {!projectFileKeywordActive && projectFileParent && (
                  <button
                    type="button"
                    className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
                    onClick={() => onLoadProjectFileEntries(projectFileParent)}
                  >
                    ..
                  </button>
                )}
                {displayedProjectFileEntries.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent flex items-center justify-between gap-2"
                    onClick={() => onAttachProjectFile(entry)}
                    disabled={projectFileAttachingPath !== null}
                  >
                    <span className="min-w-0 flex-1 truncate">
                      <span className="inline-flex items-center gap-1.5 min-w-0 max-w-full">
                        {entry.isDir ? (
                          <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                          </svg>
                        ) : (
                          <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M7 3h7l5 5v13a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1z" />
                            <path strokeLinecap="round" strokeLinejoin="round" d="M14 3v6h6" />
                          </svg>
                        )}
                        <span className="truncate">{entry.name}</span>
                      </span>
                      {projectFileKeywordActive && !entry.isDir && (
                        <span className="block truncate text-[11px] text-muted-foreground">
                          {toRelativeProjectPath(entry.path)}
                        </span>
                      )}
                    </span>
                    {projectFileAttachingPath === entry.path && (
                      <span className="text-[11px] text-muted-foreground">处理中...</span>
                    )}
                  </button>
                ))}
                {displayedProjectFileEntries.length === 0 && !projectFileBusy && (
                  <div className="px-3 py-2 text-xs text-muted-foreground">
                    {projectFileKeywordActive ? '没有匹配的文件' : '当前目录没有可选文件'}
                  </div>
                )}
                {projectFileKeywordActive && projectFileSearchTruncated && (
                  <div className="px-3 py-2 text-[11px] text-muted-foreground border-t">
                    结果过多，已截断显示前 300 条
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

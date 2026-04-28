import React from 'react';

import type { ProjectChangeSummary } from '../../../types';
import { cn } from '../../../lib/utils';
import {
  CHANGE_KIND_COLOR_CLASS,
  CHANGE_KIND_LABEL,
  CHANGE_KIND_TEXT_CLASS,
} from '../utils';

interface ProjectTreeChangeMarkSectionsProps {
  selectedPath: string | null;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  hiddenFileMarks: ProjectChangeSummary['fileMarks'];
  normalizePath: (value: string) => string;
  onSelectDeletedPath: (path: string) => void;
  onSelectMarkedPath: (path: string) => void;
}

export const ProjectTreeChangeMarkSections: React.FC<ProjectTreeChangeMarkSectionsProps> = ({
  selectedPath,
  showOnlyChanged,
  changeSummary,
  hiddenFileMarks,
  normalizePath,
  onSelectDeletedPath,
  onSelectMarkedPath,
}) => (
  <>
    {changeSummary.deletedMarks.length > 0 && (
      <div className="mt-2 border-t border-border/70">
        <div className="px-3 py-2 text-[11px] font-medium text-rose-600 dark:text-rose-400">
          已删除（未确认）
        </div>
        <div className="space-y-0.5 pb-2">
          {changeSummary.deletedMarks.map((mark) => {
            const normalizedMarkPath = normalizePath(mark.path);
            const isActive = selectedPath ? normalizePath(selectedPath) === normalizedMarkPath : false;
            return (
              <button
                key={mark.lastChangeId || mark.path}
                type="button"
                onClick={() => onSelectDeletedPath(mark.path)}
                className={cn(
                  'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 rounded py-1.5 pr-2 text-left transition-colors hover:bg-accent',
                  isActive && 'bg-accent',
                )}
                style={{ paddingLeft: 12 + 14 }}
              >
                <span className="w-3 shrink-0 text-xs text-rose-500">•</span>
                <span className={cn('truncate whitespace-nowrap text-sm', CHANGE_KIND_TEXT_CLASS.delete)}>
                  {mark.relativePath || mark.path}
                </span>
                <span className="whitespace-nowrap text-right text-[11px] tabular-nums text-muted-foreground">
                  已删除
                </span>
              </button>
            );
          })}
        </div>
      </div>
    )}
    {showOnlyChanged && hiddenFileMarks.length > 0 && (
      <div className="mt-2 border-t border-border/70">
        <div className="px-3 py-2 text-[11px] font-medium text-amber-600 dark:text-amber-400">
          未在当前目录树显示（未确认）
        </div>
        <div className="space-y-0.5 pb-2">
          {hiddenFileMarks.map((mark) => {
            const normalizedMarkPath = normalizePath(mark.path);
            const isActive = selectedPath ? normalizePath(selectedPath) === normalizedMarkPath : false;
            return (
              <button
                key={mark.lastChangeId || mark.path}
                type="button"
                onClick={() => onSelectMarkedPath(mark.path)}
                className={cn(
                  'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 rounded py-1.5 pr-2 text-left transition-colors hover:bg-accent',
                  isActive && 'bg-accent',
                )}
                style={{ paddingLeft: 12 + 14 }}
              >
                <span className={cn('inline-block h-2 w-2 rounded-full', CHANGE_KIND_COLOR_CLASS[mark.kind])} />
                <span className={cn('truncate whitespace-nowrap text-sm', CHANGE_KIND_TEXT_CLASS[mark.kind])}>
                  {mark.relativePath || mark.path}
                </span>
                <span className="whitespace-nowrap text-right text-[11px] tabular-nums text-muted-foreground">
                  {CHANGE_KIND_LABEL[mark.kind]}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    )}
  </>
);

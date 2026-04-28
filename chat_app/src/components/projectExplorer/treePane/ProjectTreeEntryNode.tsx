import React from 'react';

import type { FsEntry } from '../../../types';
import { cn, formatFileSize } from '../../../lib/utils';
import {
  CHANGE_KIND_COLOR_CLASS,
  CHANGE_KIND_LABEL,
  CHANGE_KIND_ROW_CLASS,
  CHANGE_KIND_TEXT_CLASS,
} from '../utils';
import type { ChangeKind } from '../utils';

interface ProjectTreeEntryNodeProps {
  entry: FsEntry;
  depth: number;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  selectedPath: string | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  isEntryVisible: (entryPath: string) => boolean;
  onToggleDir: (entry: FsEntry) => void;
  onOpenFile: (entry: FsEntry) => void;
  onOpenContextMenu: (event: React.MouseEvent, entry: FsEntry) => void;
  onDragStart: (event: React.DragEvent, entry: FsEntry) => void;
  onDragEnd: () => void;
  onSetDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  onSetDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  onMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => void;
  onScheduleDragExpand: (path: string) => void;
  onCancelDragExpandIfMatches: (path: string) => void;
  onClearDragAutoScroll: () => void;
}

export const ProjectTreeEntryNode: React.FC<ProjectTreeEntryNodeProps> = ({
  entry,
  depth,
  entriesMap,
  expandedPaths,
  selectedPath,
  draggingEntryPath,
  dropTargetDirPath,
  aggregatedChangeKindByPath,
  normalizePath,
  toExpandedKey,
  canDropToDirectory,
  isEntryVisible,
  onToggleDir,
  onOpenFile,
  onOpenContextMenu,
  onDragStart,
  onDragEnd,
  onSetDropTargetDirPath,
  onSetDraggingEntryPath,
  onMoveEntryByDrop,
  onScheduleDragExpand,
  onCancelDragExpandIfMatches,
  onClearDragAutoScroll,
}) => {
  const entryKey = toExpandedKey(entry.path);
  const normalizedEntryPath = normalizePath(entry.path);
  const isActive = selectedPath ? normalizePath(selectedPath) === normalizedEntryPath : false;
  const isDragging = draggingEntryPath ? normalizePath(draggingEntryPath) === normalizedEntryPath : false;
  const isDropTarget = entry.isDir && dropTargetDirPath
    ? normalizePath(dropTargetDirPath) === normalizedEntryPath
    : false;
  const entryChangeKind = aggregatedChangeKindByPath.get(normalizedEntryPath);
  const childEntries = entry.isDir && expandedPaths.has(entryKey)
    ? (entriesMap[entry.path] || []).filter((child) => isEntryVisible(child.path))
    : [];

  return (
    <div>
      <button
        type="button"
        onClick={() => (entry.isDir ? onToggleDir(entry) : onOpenFile(entry))}
        onContextMenu={(event) => onOpenContextMenu(event, entry)}
        draggable
        onDragStart={(event) => onDragStart(event, entry)}
        onDragEnd={onDragEnd}
        onDragOver={(event) => {
          if (!entry.isDir) return;
          const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
          if (!sourcePath || !canDropToDirectory(sourcePath, entry.path)) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = 'move';
        }}
        onDragEnter={(event) => {
          if (!entry.isDir) return;
          const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
          if (!sourcePath || !canDropToDirectory(sourcePath, entry.path)) return;
          event.preventDefault();
          onSetDropTargetDirPath(entry.path);
          onScheduleDragExpand(entry.path);
        }}
        onDragLeave={(event) => {
          if (!entry.isDir) return;
          const nextTarget = event.relatedTarget as Node | null;
          if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
            return;
          }
          onCancelDragExpandIfMatches(entry.path);
          onClearDragAutoScroll();
          onSetDropTargetDirPath((prev) => (
            prev && normalizePath(prev) === normalizePath(entry.path) ? null : prev
          ));
        }}
        onDrop={(event) => {
          if (!entry.isDir) return;
          const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
          if (!sourcePath) return;
          if (!canDropToDirectory(sourcePath, entry.path)) return;
          event.preventDefault();
          event.stopPropagation();
          onCancelDragExpandIfMatches(entry.path);
          onClearDragAutoScroll();
          onSetDropTargetDirPath(null);
          onSetDraggingEntryPath(null);
          onMoveEntryByDrop(sourcePath, entry.path);
        }}
        className={cn(
          'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 rounded py-1.5 pr-2 text-left transition-colors hover:bg-accent',
          entryChangeKind && CHANGE_KIND_ROW_CLASS[entryChangeKind],
          isActive && 'bg-accent',
          isDragging && 'opacity-50',
          isDropTarget && 'bg-blue-500/10 ring-1 ring-blue-500',
        )}
        style={{ paddingLeft: 12 + depth * 14 }}
      >
        <span className="w-3 shrink-0 text-xs text-muted-foreground">
          {entry.isDir ? (expandedPaths.has(entryKey) ? '▾' : '▸') : ''}
        </span>
        <span
          className={cn(
            'inline-flex items-center gap-1 whitespace-nowrap text-sm',
            entry.isDir ? 'text-foreground' : 'text-muted-foreground',
            entryChangeKind && CHANGE_KIND_TEXT_CLASS[entryChangeKind],
          )}
        >
          {entry.name}
          {entryChangeKind && (
            <span
              className={cn('inline-block h-2 w-2 rounded-full', CHANGE_KIND_COLOR_CLASS[entryChangeKind])}
              title={`未确认${CHANGE_KIND_LABEL[entryChangeKind]}变更`}
            />
          )}
        </span>
        <span className="whitespace-nowrap text-right text-[11px] tabular-nums text-muted-foreground">
          {!entry.isDir && entry.size != null ? formatFileSize(entry.size) : ''}
        </span>
      </button>
      {childEntries.map((child) => (
        <ProjectTreeEntryNode
          key={child.path}
          entry={child}
          depth={depth + 1}
          entriesMap={entriesMap}
          expandedPaths={expandedPaths}
          selectedPath={selectedPath}
          draggingEntryPath={draggingEntryPath}
          dropTargetDirPath={dropTargetDirPath}
          aggregatedChangeKindByPath={aggregatedChangeKindByPath}
          normalizePath={normalizePath}
          toExpandedKey={toExpandedKey}
          canDropToDirectory={canDropToDirectory}
          isEntryVisible={isEntryVisible}
          onToggleDir={onToggleDir}
          onOpenFile={onOpenFile}
          onOpenContextMenu={onOpenContextMenu}
          onDragStart={onDragStart}
          onDragEnd={onDragEnd}
          onSetDropTargetDirPath={onSetDropTargetDirPath}
          onSetDraggingEntryPath={onSetDraggingEntryPath}
          onMoveEntryByDrop={onMoveEntryByDrop}
          onScheduleDragExpand={onScheduleDragExpand}
          onCancelDragExpandIfMatches={onCancelDragExpandIfMatches}
          onClearDragAutoScroll={onClearDragAutoScroll}
        />
      ))}
    </div>
  );
};

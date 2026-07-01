// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { FsEntry, Project, ProjectSearchHit } from '../../../types';
import { ProjectTreeEntryNode } from './ProjectTreeEntryNode';
import { ProjectTreeSearchResults } from './ProjectTreeSearchResults';
import { useProjectTreeAutoScrollHandlers } from './useProjectTreeAutoScrollHandlers';

interface ProjectTreeEntriesProps {
  project: Project;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedPath: string | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  onOpenSearchHit: (hit: ProjectSearchHit) => void;
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
  onStartDragAutoScroll: (velocity: number) => void;
  onClearDragAutoScroll: () => void;
}

export const ProjectTreeEntries: React.FC<ProjectTreeEntriesProps> = ({
  project,
  treeScrollRef,
  entriesMap,
  expandedPaths,
  loadingPaths,
  selectedPath,
  draggingEntryPath,
  dropTargetDirPath,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  normalizePath,
  toExpandedKey,
  canDropToDirectory,
  onOpenSearchHit,
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
  onStartDragAutoScroll,
  onClearDragAutoScroll,
}) => {
  const { t } = useI18n();
  const rootEntries = entriesMap[project.rootPath] || [];
  const {
    handleContainerDragLeave,
    handleContainerDragOver,
    handleContainerDrop,
  } = useProjectTreeAutoScrollHandlers({
    draggingEntryPath,
    treeScrollRef,
    onStartDragAutoScroll,
    onClearDragAutoScroll,
  });

  return (
    <div
      ref={treeScrollRef}
      className="flex-1 overflow-y-auto overflow-x-auto py-2"
      onDragOver={handleContainerDragOver}
      onDragLeave={handleContainerDragLeave}
      onDrop={handleContainerDrop}
    >
      {searchQuery.trim().length > 0 ? (
        <ProjectTreeSearchResults
          searchQuery={searchQuery}
          searchCaseSensitive={searchCaseSensitive}
          searchWholeWord={searchWholeWord}
          searchResults={searchResults}
          activeSearchHitId={activeSearchHitId}
          onOpenSearchHit={onOpenSearchHit}
        />
      ) : (
        <>
          {rootEntries.map((entry) => (
            <ProjectTreeEntryNode
              key={entry.path}
              entry={entry}
              depth={0}
              entriesMap={entriesMap}
              expandedPaths={expandedPaths}
              selectedPath={selectedPath}
              draggingEntryPath={draggingEntryPath}
              dropTargetDirPath={dropTargetDirPath}
              normalizePath={normalizePath}
              toExpandedKey={toExpandedKey}
              canDropToDirectory={canDropToDirectory}
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
          {loadingPaths.has(project.rootPath) && (
            <div className="px-3 py-2 text-xs text-muted-foreground">{t('common.loading')}</div>
          )}
          {!loadingPaths.has(project.rootPath) && rootEntries.length === 0 && (
            <div className="px-3 py-2 text-xs text-muted-foreground">{t('projectExplorer.tree.emptyDirectory')}</div>
          )}
        </>
      )}
    </div>
  );
};

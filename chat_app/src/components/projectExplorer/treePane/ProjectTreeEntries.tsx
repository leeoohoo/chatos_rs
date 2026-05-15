import React from 'react';

import type { FsEntry, Project, ProjectChangeSummary, ProjectSearchHit } from '../../../types';
import type { ChangeKind } from '../utils';
import { ProjectTreeChangeMarkSections } from './ProjectTreeChangeMarkSections';
import { ProjectTreeEntryNode } from './ProjectTreeEntryNode';
import { ProjectTreeSearchResults } from './ProjectTreeSearchResults';
import { useProjectTreeAutoScrollHandlers } from './useProjectTreeAutoScrollHandlers';
import { useProjectTreeEntriesDerivedState } from './useProjectTreeEntriesDerivedState';

interface ProjectTreeEntriesProps {
  project: Project;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedPath: string | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  onOpenSearchHit: (hit: ProjectSearchHit) => void;
  onSelectDeletedPath: (path: string) => void;
  onSelectMarkedPath: (path: string) => void;
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
  showOnlyChanged,
  changeSummary,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  aggregatedChangeKindByPath,
  normalizePath,
  toExpandedKey,
  canDropToDirectory,
  onOpenSearchHit,
  onSelectDeletedPath,
  onSelectMarkedPath,
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
  const {
    hiddenFileMarks,
    isEntryVisible,
    visibleRootEntryCount,
  } = useProjectTreeEntriesDerivedState({
    projectRootPath: project.rootPath,
    entriesMap,
    showOnlyChanged,
    changeSummary,
    aggregatedChangeKindByPath,
    normalizePath,
  });

  const rootEntries = (entriesMap[project.rootPath] || []).filter((entry) => isEntryVisible(entry.path));
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
          <ProjectTreeChangeMarkSections
            selectedPath={selectedPath}
            showOnlyChanged={showOnlyChanged}
            changeSummary={changeSummary}
            hiddenFileMarks={hiddenFileMarks}
            normalizePath={normalizePath}
            onSelectDeletedPath={onSelectDeletedPath}
            onSelectMarkedPath={onSelectMarkedPath}
          />
          {loadingPaths.has(project.rootPath) && (
            <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
          )}
          {!loadingPaths.has(project.rootPath) && visibleRootEntryCount === 0 && (
            <div className="px-3 py-2 text-xs text-muted-foreground">
              {showOnlyChanged
                ? (changeSummary.counts.total > 0
                  ? '存在变更，但当前目录树未命中。请查看下方列表。'
                  : '暂无变更文件')
                : '目录为空'}
            </div>
          )}
        </>
      )}
    </div>
  );
};

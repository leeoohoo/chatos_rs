import React from 'react';

import type { FsEntry, FsReadResult, ProjectSearchHit } from '../../../types';
import {
  ProjectPreviewBinaryDownload,
  ProjectPreviewDeletedPathState,
  ProjectPreviewEmptyState,
  ProjectPreviewImageContent,
  ProjectPreviewLoadingState,
} from './ProjectPreviewFileStates';
import { ProjectPreviewTextContent } from './ProjectPreviewTextContent';
import type { PreviewTokenSelection } from './previewPaneTypes';

interface ProjectPreviewFileContentProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  targetLine: number | null;
  targetLineRevision: number;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  onActivateSearchHit: (hit: ProjectSearchHit) => void;
  onTokenSelection: (selection: PreviewTokenSelection | null) => void;
}

export const ProjectPreviewFileContent: React.FC<ProjectPreviewFileContentProps> = ({
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  targetLine,
  targetLineRevision,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  onActivateSearchHit,
  onTokenSelection,
}) => {
  if (loadingFile) {
    return <ProjectPreviewLoadingState />;
  }
  if (!selectedFile) {
    return selectedPath && !selectedEntry
      ? <ProjectPreviewDeletedPathState />
      : <ProjectPreviewEmptyState />;
  }

  const isImage = selectedFile.contentType.startsWith('image/');
  if (isImage && selectedFile.isBinary) {
    return <ProjectPreviewImageContent selectedFile={selectedFile} />;
  }
  if (selectedFile.isBinary) {
    return <ProjectPreviewBinaryDownload selectedFile={selectedFile} />;
  }

  return (
    <ProjectPreviewTextContent
      selectedFile={selectedFile}
      selectedPath={selectedPath}
      targetLine={targetLine}
      targetLineRevision={targetLineRevision}
      searchQuery={searchQuery}
      searchCaseSensitive={searchCaseSensitive}
      searchWholeWord={searchWholeWord}
      searchResults={searchResults}
      activeSearchHitId={activeSearchHitId}
      onActivateSearchHit={onActivateSearchHit}
      onTokenSelection={onTokenSelection}
    />
  );
};

import type { Dispatch, SetStateAction } from 'react';

import type {
  FsMoveOptions,
  FsMoveResponse,
  FsMutationResponse,
} from '../../lib/api/client/types';
import type { FsEntry, FsReadResult } from '../../types';
import type { MoveConflictState } from './Overlays';

export interface ProjectTreeActionsClient {
  createFsDirectory(parentPath: string, name: string): Promise<FsMutationResponse>;
  createFsFile(parentPath: string, name: string, content?: string): Promise<FsMutationResponse>;
  deleteFsEntry(path: string, recursive?: boolean): Promise<FsMutationResponse>;
  downloadFsEntry(path: string): Promise<{ blob: Blob; filename: string; contentType: string }>;
  moveFsEntry(
    sourcePath: string,
    targetParentPath: string,
    options?: FsMoveOptions,
  ): Promise<FsMoveResponse>;
}

export interface UseProjectTreeActionsOptions {
  client: ProjectTreeActionsClient;
  selectedDirPath: string | null;
  selectedEntry: FsEntry | null;
  selectedFilePath: string | null;
  projectRootPath?: string | null;
  actionReloadPath: string | null;
  normalizePath: (value: string) => string;
  getParentPath: (value: string) => string | null;
  toExpandedKey: (path: string) => string;
  loadEntries: (path: string) => Promise<void>;
  loadChangeSummary: (options?: { silent?: boolean }) => Promise<void>;
  pruneDeletedPath: (deletedPath: string) => void;
  replaceExpandedPathPrefix: (sourcePath: string, movedPath: string) => Set<string>;
  reloadTreeWithExpanded: (nextExpanded: Set<string>) => Promise<void>;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  findEntryByPath: (path: string) => FsEntry | null;
  clearDragExpandTimer: () => void;
  clearDragAutoScroll: () => void;
  setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
  setSelectedPath: Dispatch<SetStateAction<string | null>>;
  setSelectedFile: Dispatch<SetStateAction<FsReadResult | null>>;
  setActionLoading: Dispatch<SetStateAction<boolean>>;
  setActionError: Dispatch<SetStateAction<string | null>>;
  setActionMessage: Dispatch<SetStateAction<string | null>>;
  setMoveConflict: Dispatch<SetStateAction<MoveConflictState | null>>;
  openFile: (entry: FsEntry) => Promise<void>;
}

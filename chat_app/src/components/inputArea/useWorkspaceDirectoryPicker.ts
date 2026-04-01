import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import type { FsEntry } from '../../types';
import { normalizeFsEntry } from './fileUtils';

interface WorkspaceApiClient {
  listFsDirectories: (path?: string) => Promise<unknown>;
}

interface WorkspaceDirectoryResponse {
  path?: string | null;
  parent?: string | null;
  entries?: unknown[];
  roots?: unknown[];
}

interface UseWorkspaceDirectoryPickerOptions {
  client: WorkspaceApiClient;
  showWorkspaceRootPicker: boolean;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  normalizedWorkspaceRoot: string | null;
  onWorkspaceRootChange?: (path: string | null) => void;
}

interface UseWorkspaceDirectoryPickerResult {
  workspacePickerOpen: boolean;
  setWorkspacePickerOpen: Dispatch<SetStateAction<boolean>>;
  workspacePath: string | null;
  workspaceParent: string | null;
  workspaceEntries: FsEntry[];
  workspaceRoots: FsEntry[];
  workspaceLoading: boolean;
  workspaceError: string | null;
  loadWorkspaceDirectories: (nextPath?: string | null) => Promise<void>;
  handleToggleWorkspacePicker: () => Promise<void>;
  handleSelectWorkspaceRoot: (path: string | null) => void;
}

export const useWorkspaceDirectoryPicker = ({
  client,
  showWorkspaceRootPicker,
  disabled,
  isStreaming,
  isStopping,
  normalizedWorkspaceRoot,
  onWorkspaceRootChange,
}: UseWorkspaceDirectoryPickerOptions): UseWorkspaceDirectoryPickerResult => {
  const [workspacePickerOpen, setWorkspacePickerOpen] = useState(false);
  const [workspacePath, setWorkspacePath] = useState<string | null>(null);
  const [workspaceParent, setWorkspaceParent] = useState<string | null>(null);
  const [workspaceEntries, setWorkspaceEntries] = useState<FsEntry[]>([]);
  const [workspaceRoots, setWorkspaceRoots] = useState<FsEntry[]>([]);
  const [workspaceLoading, setWorkspaceLoading] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);

  const loadWorkspaceDirectories = useCallback(async (nextPath?: string | null) => {
    setWorkspaceLoading(true);
    setWorkspaceError(null);
    try {
      const data = await client.listFsDirectories(nextPath || undefined) as WorkspaceDirectoryResponse;
      const path = typeof data?.path === 'string' ? data.path : null;
      const parent = typeof data?.parent === 'string' ? data.parent : null;
      const entries = Array.isArray(data?.entries)
        ? data.entries
          .map((entry) => normalizeFsEntry(entry as Record<string, unknown>))
          .filter((entry: FsEntry) => entry.isDir)
        : [];
      const roots = Array.isArray(data?.roots)
        ? data.roots
          .map((entry) => normalizeFsEntry(entry as Record<string, unknown>))
          .filter((entry: FsEntry) => entry.isDir)
        : [];
      setWorkspacePath(path);
      setWorkspaceParent(parent);
      setWorkspaceEntries(entries);
      setWorkspaceRoots(roots);
    } catch (error) {
      setWorkspaceError(error instanceof Error ? error.message : '加载目录失败');
    } finally {
      setWorkspaceLoading(false);
    }
  }, [client]);

  const handleToggleWorkspacePicker = useCallback(async () => {
    if (!showWorkspaceRootPicker || disabled || isStreaming || isStopping) {
      return;
    }
    if (workspacePickerOpen) {
      setWorkspacePickerOpen(false);
      return;
    }
    setWorkspacePickerOpen(true);
    await loadWorkspaceDirectories(normalizedWorkspaceRoot || null);
  }, [
    disabled,
    isStopping,
    isStreaming,
    loadWorkspaceDirectories,
    normalizedWorkspaceRoot,
    showWorkspaceRootPicker,
    workspacePickerOpen,
  ]);

  const handleSelectWorkspaceRoot = useCallback((path: string | null) => {
    onWorkspaceRootChange?.(path && path.trim().length > 0 ? path : null);
    setWorkspacePickerOpen(false);
  }, [onWorkspaceRootChange]);

  useEffect(() => {
    if (!showWorkspaceRootPicker) {
      setWorkspacePickerOpen(false);
      setWorkspaceError(null);
    }
  }, [showWorkspaceRootPicker]);

  return {
    workspacePickerOpen,
    setWorkspacePickerOpen,
    workspacePath,
    workspaceParent,
    workspaceEntries,
    workspaceRoots,
    workspaceLoading,
    workspaceError,
    loadWorkspaceDirectories,
    handleToggleWorkspacePicker,
    handleSelectWorkspaceRoot,
  };
};

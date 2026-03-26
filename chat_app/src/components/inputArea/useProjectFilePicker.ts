import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import type { FsEntry } from '../../types';
import { compactSearchText, fuzzyMatch, normalizeFsEntry } from './fileUtils';

interface ProjectFileApiClient {
  listFsEntries: (path?: string) => Promise<unknown>;
  searchFsEntries: (rootPath: string, keyword: string, limit?: number) => Promise<unknown>;
  readFsFile: (path: string) => Promise<unknown>;
}

interface UseProjectFilePickerOptions {
  client: ProjectFileApiClient;
  showProjectFilePicker: boolean;
  disabled: boolean;
  projectRootForFilePicker: string | null;
  addFiles: (incoming: File[]) => void;
}

interface UseProjectFilePickerResult {
  projectFilePickerOpen: boolean;
  setProjectFilePickerOpen: Dispatch<SetStateAction<boolean>>;
  projectFileEntries: FsEntry[];
  projectFilePath: string | null;
  projectFileParent: string | null;
  projectFileFilter: string;
  setProjectFileFilter: Dispatch<SetStateAction<string>>;
  projectFileLoading: boolean;
  projectFileSearching: boolean;
  projectFileSearchResults: FsEntry[];
  projectFileSearchTruncated: boolean;
  projectFileError: string | null;
  projectFileAttachingPath: string | null;
  projectFilePathLabel: string;
  projectFileKeywordActive: boolean;
  displayedProjectFileEntries: FsEntry[];
  projectFileBusy: boolean;
  loadProjectFileEntries: (nextPath?: string | null) => Promise<void>;
  handleToggleProjectFilePicker: () => Promise<void>;
  handleAttachProjectFile: (entry: FsEntry) => Promise<void>;
  toRelativeProjectPath: (absolutePath: string) => string;
}

const normalizePath = (value: string): string => {
  const normalized = value.replace(/\\/g, '/').replace(/\/+/g, '/');
  if (normalized.length > 1 && normalized.endsWith('/')) {
    return normalized.slice(0, -1);
  }
  return normalized;
};

const isPathWithinRoot = (candidate: string, root: string): boolean => {
  const normalizedCandidate = normalizePath(candidate);
  const normalizedRoot = normalizePath(root);
  return normalizedCandidate === normalizedRoot || normalizedCandidate.startsWith(`${normalizedRoot}/`);
};

export const useProjectFilePicker = ({
  client,
  showProjectFilePicker,
  disabled,
  projectRootForFilePicker,
  addFiles,
}: UseProjectFilePickerOptions): UseProjectFilePickerResult => {
  const [projectFilePickerOpen, setProjectFilePickerOpen] = useState(false);
  const [projectFileEntries, setProjectFileEntries] = useState<FsEntry[]>([]);
  const [projectFilePath, setProjectFilePath] = useState<string | null>(null);
  const [projectFileParent, setProjectFileParent] = useState<string | null>(null);
  const [projectFileFilter, setProjectFileFilter] = useState('');
  const [projectFileLoading, setProjectFileLoading] = useState(false);
  const [projectFileSearching, setProjectFileSearching] = useState(false);
  const [projectFileSearchResults, setProjectFileSearchResults] = useState<FsEntry[]>([]);
  const [projectFileSearchTruncated, setProjectFileSearchTruncated] = useState(false);
  const [projectFileError, setProjectFileError] = useState<string | null>(null);
  const [projectFileAttachingPath, setProjectFileAttachingPath] = useState<string | null>(null);

  const isHiddenProjectPath = useCallback((candidatePath: string) => {
    if (!projectRootForFilePicker) return false;
    const normalizedCandidate = normalizePath(candidatePath || '');
    if (!normalizedCandidate) return false;
    const normalizedRoot = normalizePath(projectRootForFilePicker);
    if (!normalizedRoot) return false;

    let relativePath = normalizedCandidate;
    if (normalizedCandidate === normalizedRoot) {
      relativePath = '';
    } else if (normalizedCandidate.startsWith(`${normalizedRoot}/`)) {
      relativePath = normalizedCandidate.slice(normalizedRoot.length + 1);
    }

    if (!relativePath) return false;
    return relativePath.split('/').some((segment) => segment.startsWith('.'));
  }, [projectRootForFilePicker]);

  const filteredProjectFileEntries = useMemo(() => {
    const keywordRaw = projectFileFilter.trim().toLocaleLowerCase();
    const keywordCompact = compactSearchText(keywordRaw);
    const source = (projectFileEntries || [])
      .filter((entry) => !isHiddenProjectPath(entry.path))
      .slice()
      .sort((a, b) => {
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
    if (!keywordRaw) return source;

    const matches = (value: string) => {
      const text = value.toLocaleLowerCase();
      const compactText = compactSearchText(text);
      if (fuzzyMatch(text, keywordRaw) || fuzzyMatch(compactText, keywordCompact)) {
        return true;
      }
      return false;
    };

    return source.filter((entry) => {
      const nameText = entry.name;
      const normalizedEntryPath = normalizePath(entry.path);
      let relativePathText = normalizedEntryPath;
      if (projectRootForFilePicker) {
        const normalizedRoot = normalizePath(projectRootForFilePicker);
        const prefix = `${normalizedRoot}/`;
        if (normalizedEntryPath.startsWith(prefix)) {
          relativePathText = normalizedEntryPath.slice(prefix.length);
        }
      }
      return matches(nameText) || matches(relativePathText);
    });
  }, [isHiddenProjectPath, projectFileEntries, projectFileFilter, projectRootForFilePicker]);
  const projectFileKeywordActive = projectFileFilter.trim().length > 0;
  const displayedProjectFileEntries = projectFileKeywordActive
    ? projectFileSearchResults
    : filteredProjectFileEntries;
  const projectFileBusy = projectFileKeywordActive ? projectFileSearching : projectFileLoading;

  const toRelativeProjectPath = useCallback((absolutePath: string) => {
    if (!projectRootForFilePicker) return absolutePath;
    const normalized = normalizePath(absolutePath);
    if (normalized === projectRootForFilePicker) {
      return absolutePath;
    }
    const prefix = `${projectRootForFilePicker}/`;
    if (normalized.startsWith(prefix)) {
      return normalized.slice(prefix.length);
    }
    return normalized;
  }, [projectRootForFilePicker]);

  const projectFilePathLabel = useMemo(() => {
    if (!projectFilePath || !projectRootForFilePicker) return '';
    const normalized = normalizePath(projectFilePath);
    if (normalized === projectRootForFilePicker) return '/';
    const prefix = `${projectRootForFilePicker}/`;
    if (normalized.startsWith(prefix)) {
      return `/${normalized.slice(prefix.length)}`;
    }
    return normalized;
  }, [projectFilePath, projectRootForFilePicker]);

  useEffect(() => {
    setProjectFilePickerOpen(false);
    setProjectFileEntries([]);
    setProjectFileSearchResults([]);
    setProjectFileSearchTruncated(false);
    setProjectFileSearching(false);
    setProjectFilePath(null);
    setProjectFileParent(null);
    setProjectFileFilter('');
    setProjectFileError(null);
    setProjectFileAttachingPath(null);
  }, [projectRootForFilePicker]);

  useEffect(() => {
    if (!projectFilePickerOpen || !projectRootForFilePicker) return;

    const keyword = projectFileFilter.trim();
    if (!keyword) {
      setProjectFileSearchResults([]);
      setProjectFileSearchTruncated(false);
      setProjectFileSearching(false);
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(async () => {
      setProjectFileSearching(true);
      setProjectFileError(null);
      try {
        const data: any = await client.searchFsEntries(projectRootForFilePicker, keyword, 300);
        if (cancelled) return;

        const entriesRaw: any[] = Array.isArray(data?.entries) ? data.entries : [];
        const normalizedEntries = entriesRaw
          .map((raw: any) => normalizeFsEntry(raw))
          .filter((entry: FsEntry) => (
            !entry.isDir
            && entry.path
            && isPathWithinRoot(entry.path, projectRootForFilePicker)
            && !isHiddenProjectPath(entry.path)
          ));

        setProjectFileSearchResults(normalizedEntries);
        setProjectFileSearchTruncated(Boolean(data?.truncated));
      } catch (error: any) {
        if (cancelled) return;
        setProjectFileError(error?.message || '搜索项目文件失败');
        setProjectFileSearchResults([]);
        setProjectFileSearchTruncated(false);
      } finally {
        if (!cancelled) {
          setProjectFileSearching(false);
        }
      }
    }, 150);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [client, isHiddenProjectPath, projectFileFilter, projectFilePickerOpen, projectRootForFilePicker]);

  const loadProjectFileEntries = useCallback(async (nextPath?: string | null) => {
    if (!projectRootForFilePicker) return;

    const fallbackRoot = normalizePath(projectRootForFilePicker);
    const preferredPath = nextPath ? normalizePath(nextPath) : fallbackRoot;
    let safePath = isPathWithinRoot(preferredPath, fallbackRoot) ? preferredPath : fallbackRoot;
    if (isHiddenProjectPath(safePath)) {
      safePath = fallbackRoot;
    }

    setProjectFileLoading(true);
    setProjectFileError(null);
    try {
      const data: any = await client.listFsEntries(safePath);
      const currentPathRaw = typeof data?.path === 'string' && data.path ? data.path : safePath;
      const normalizedCurrent = normalizePath(currentPathRaw);
      const entriesRaw: any[] = Array.isArray(data?.entries) ? data.entries : [];
      const normalizedEntries = entriesRaw
        .map((raw: any) => normalizeFsEntry(raw))
        .filter((entry: FsEntry) => (
          entry.path
          && isPathWithinRoot(entry.path, fallbackRoot)
          && !isHiddenProjectPath(entry.path)
        ));
      const parentRaw = typeof data?.parent === 'string' ? normalizePath(data.parent) : null;

      setProjectFilePath(isPathWithinRoot(normalizedCurrent, fallbackRoot) ? normalizedCurrent : fallbackRoot);
      if (parentRaw && isPathWithinRoot(parentRaw, fallbackRoot) && parentRaw !== fallbackRoot && !isHiddenProjectPath(parentRaw)) {
        setProjectFileParent(parentRaw);
      } else {
        setProjectFileParent(null);
      }
      setProjectFileEntries(normalizedEntries);
    } catch (error: any) {
      setProjectFileError(error?.message || '加载项目文件失败');
      setProjectFileEntries([]);
    } finally {
      setProjectFileLoading(false);
    }
  }, [client, isHiddenProjectPath, projectRootForFilePicker]);

  const handleToggleProjectFilePicker = useCallback(async () => {
    if (!showProjectFilePicker || disabled) return;

    if (projectFilePickerOpen) {
      setProjectFilePickerOpen(false);
      return;
    }

    const initialPath = projectFilePath && projectRootForFilePicker && isPathWithinRoot(projectFilePath, projectRootForFilePicker)
      ? projectFilePath
      : projectRootForFilePicker;

    setProjectFilePickerOpen(true);
    setProjectFileFilter('');
    await loadProjectFileEntries(initialPath);
  }, [
    disabled,
    loadProjectFileEntries,
    projectFilePath,
    projectFilePickerOpen,
    projectRootForFilePicker,
    showProjectFilePicker,
  ]);

  const handleAttachProjectFile = useCallback(async (entry: FsEntry) => {
    if (!projectRootForFilePicker) return;

    if (entry.isDir) {
      await loadProjectFileEntries(entry.path);
      return;
    }

    setProjectFileAttachingPath(entry.path);
    setProjectFileError(null);
    try {
      const rawFile: any = await client.readFsFile(entry.path);
      const isBinary = rawFile?.is_binary ?? rawFile?.isBinary;
      if (isBinary) {
        throw new Error('暂不支持二进制文件，请选择文本文件');
      }
      const content = typeof rawFile?.content === 'string' ? rawFile.content : '';
      const rawContentType = String(rawFile?.content_type || rawFile?.contentType || '').trim().toLowerCase();
      const normalizedContentType = (
        rawContentType.startsWith('text/')
        || rawContentType === 'application/json'
        || rawContentType === 'application/pdf'
        || rawContentType === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
      )
        ? rawContentType
        : 'text/plain';
      const relativePath = toRelativeProjectPath(entry.path) || entry.name;
      const fileToAttach = new File([content], relativePath, { type: normalizedContentType });
      addFiles([fileToAttach]);
      setProjectFilePickerOpen(false);
    } catch (error: any) {
      const rawMessage = error?.message || '读取项目文件失败';
      if (String(rawMessage).includes('413')) {
        setProjectFileError('文件过大，当前最多支持 2MB 的项目文件');
      } else {
        setProjectFileError(rawMessage);
      }
    } finally {
      setProjectFileAttachingPath(null);
    }
  }, [addFiles, client, loadProjectFileEntries, projectRootForFilePicker, toRelativeProjectPath]);

  return {
    projectFilePickerOpen,
    setProjectFilePickerOpen,
    projectFileEntries,
    projectFilePath,
    projectFileParent,
    projectFileFilter,
    setProjectFileFilter,
    projectFileLoading,
    projectFileSearching,
    projectFileSearchResults,
    projectFileSearchTruncated,
    projectFileError,
    projectFileAttachingPath,
    projectFilePathLabel,
    projectFileKeywordActive,
    displayedProjectFileEntries,
    projectFileBusy,
    loadProjectFileEntries,
    handleToggleProjectFilePicker,
    handleAttachProjectFile,
    toRelativeProjectPath,
  };
};

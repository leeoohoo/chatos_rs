import { useCallback, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import type { FsEntry } from '../../types';
import {
  deriveParentPath,
  getKeyFilePickerTitle,
  normalizeFsEntry,
  type DirPickerTarget,
  type KeyFilePickerTarget,
} from './helpers';

interface FsPickerApiClient {
  listFsDirectories: (path?: string) => Promise<any>;
  createFsDirectory: (basePath: string, name: string) => Promise<any>;
  listFsEntries: (path?: string) => Promise<any>;
}

interface UseLocalFsPickersOptions {
  apiClient: FsPickerApiClient;
  projectRoot: string;
  terminalRoot: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteJumpPrivateKeyPath: string;
  onProjectRootChange: (path: string) => void;
  onTerminalRootChange: (path: string) => void;
  onRemotePrivateKeyPathChange: (path: string) => void;
  onRemoteCertificatePathChange: (path: string) => void;
  onRemoteJumpPrivateKeyPathChange: (path: string) => void;
}

interface UseLocalFsPickersResult {
  keyFilePickerOpen: boolean;
  keyFilePickerTitle: string;
  keyFilePickerPath: string | null;
  keyFilePickerParent: string | null;
  keyFilePickerLoading: boolean;
  keyFilePickerItems: FsEntry[];
  keyFilePickerError: string | null;
  dirPickerOpen: boolean;
  dirPickerTarget: DirPickerTarget;
  dirPickerPath: string | null;
  dirPickerParent: string | null;
  dirPickerLoading: boolean;
  dirPickerItems: FsEntry[];
  dirPickerError: string | null;
  showHiddenDirs: boolean;
  dirPickerCreateModalOpen: boolean;
  dirPickerNewFolderName: string;
  dirPickerCreatingFolder: boolean;
  setShowHiddenDirs: Dispatch<SetStateAction<boolean>>;
  setDirPickerCreateModalOpen: Dispatch<SetStateAction<boolean>>;
  setDirPickerNewFolderName: Dispatch<SetStateAction<string>>;
  openDirPicker: (target: DirPickerTarget) => Promise<void>;
  closeDirPicker: () => void;
  openCreateDirModal: () => void;
  createDirInPicker: () => Promise<void>;
  chooseDir: (path: string | null) => void;
  openKeyFilePicker: (target: KeyFilePickerTarget) => Promise<void>;
  closeKeyFilePicker: () => void;
  applySelectedKeyFile: (path: string) => void;
  loadDirEntries: (path?: string | null) => Promise<void>;
  loadKeyFileEntries: (path?: string | null) => Promise<void>;
  setKeyFilePickerOpen: Dispatch<SetStateAction<boolean>>;
}

export const useLocalFsPickers = ({
  apiClient,
  projectRoot,
  terminalRoot,
  remotePrivateKeyPath,
  remoteCertificatePath,
  remoteJumpPrivateKeyPath,
  onProjectRootChange,
  onTerminalRootChange,
  onRemotePrivateKeyPathChange,
  onRemoteCertificatePathChange,
  onRemoteJumpPrivateKeyPathChange,
}: UseLocalFsPickersOptions): UseLocalFsPickersResult => {
  const [keyFilePickerOpen, setKeyFilePickerOpen] = useState(false);
  const [keyFilePickerTarget, setKeyFilePickerTarget] = useState<KeyFilePickerTarget>('private_key');
  const [keyFilePickerPath, setKeyFilePickerPath] = useState<string | null>(null);
  const [keyFilePickerParent, setKeyFilePickerParent] = useState<string | null>(null);
  const [keyFilePickerEntries, setKeyFilePickerEntries] = useState<FsEntry[]>([]);
  const [keyFilePickerRoots, setKeyFilePickerRoots] = useState<FsEntry[]>([]);
  const [keyFilePickerLoading, setKeyFilePickerLoading] = useState(false);
  const [keyFilePickerError, setKeyFilePickerError] = useState<string | null>(null);

  const [dirPickerOpen, setDirPickerOpen] = useState(false);
  const [dirPickerTarget, setDirPickerTarget] = useState<DirPickerTarget>('project');
  const [dirPickerPath, setDirPickerPath] = useState<string | null>(null);
  const [dirPickerParent, setDirPickerParent] = useState<string | null>(null);
  const [dirPickerEntries, setDirPickerEntries] = useState<FsEntry[]>([]);
  const [dirPickerRoots, setDirPickerRoots] = useState<FsEntry[]>([]);
  const [dirPickerLoading, setDirPickerLoading] = useState(false);
  const [dirPickerError, setDirPickerError] = useState<string | null>(null);
  const [showHiddenDirs, setShowHiddenDirs] = useState(false);
  const [dirPickerNewFolderName, setDirPickerNewFolderName] = useState('');
  const [dirPickerCreatingFolder, setDirPickerCreatingFolder] = useState(false);
  const [dirPickerCreateModalOpen, setDirPickerCreateModalOpen] = useState(false);

  const loadDirEntries = useCallback(async (path?: string | null) => {
    setDirPickerLoading(true);
    setDirPickerError(null);
    try {
      const data = await apiClient.listFsDirectories(path || undefined);
      setDirPickerPath(data?.path ?? null);
      setDirPickerParent(data?.parent ?? null);
      setDirPickerEntries(
        Array.isArray(data?.entries)
          ? data.entries.map((entry: any) => normalizeFsEntry(entry, true))
          : [],
      );
      setDirPickerRoots(
        Array.isArray(data?.roots)
          ? data.roots.map((entry: any) => normalizeFsEntry(entry, true))
          : [],
      );
    } catch (err: any) {
      setDirPickerError(err?.message || '加载目录失败');
    } finally {
      setDirPickerLoading(false);
    }
  }, [apiClient]);

  const openDirPicker = useCallback(async (target: DirPickerTarget) => {
    setDirPickerTarget(target);
    setShowHiddenDirs(false);
    setDirPickerNewFolderName('');
    setDirPickerCreateModalOpen(false);
    setDirPickerError(null);
    setDirPickerOpen(true);
    const current = (target === 'project' ? projectRoot : terminalRoot).trim();
    await loadDirEntries(current ? current : null);
  }, [loadDirEntries, projectRoot, terminalRoot]);

  const closeDirPicker = useCallback(() => {
    setDirPickerOpen(false);
    setDirPickerCreateModalOpen(false);
    setDirPickerNewFolderName('');
  }, []);

  const openCreateDirModal = useCallback(() => {
    if (!dirPickerPath) {
      setDirPickerError('请先进入一个父目录后再新建目录');
      return;
    }
    setDirPickerError(null);
    setDirPickerNewFolderName('');
    setDirPickerCreateModalOpen(true);
  }, [dirPickerPath]);

  const createDirInPicker = useCallback(async () => {
    const basePath = dirPickerPath;
    if (!basePath) {
      setDirPickerError('请先进入一个父目录后再新建目录');
      return;
    }
    const name = dirPickerNewFolderName.trim();
    if (!name) {
      setDirPickerError('请输入新目录名称');
      return;
    }

    setDirPickerCreatingFolder(true);
    setDirPickerError(null);
    try {
      const data = await apiClient.createFsDirectory(basePath, name);

      const apiPath = typeof data?.path === 'string' ? data.path.trim() : '';
      const fallbackSep = basePath.includes('\\') && !basePath.includes('/') ? '\\' : '/';
      const normalizedBase = basePath.replace(/[\\/]+$/, '');
      const createdPath = apiPath || `${normalizedBase}${fallbackSep}${name}`;

      setDirPickerNewFolderName('');
      setDirPickerCreateModalOpen(false);

      if (dirPickerTarget === 'project') {
        onProjectRootChange(createdPath);
      } else {
        onTerminalRootChange(createdPath);
      }

      await loadDirEntries(createdPath);
    } catch (err: any) {
      setDirPickerError(err?.message || '新建目录失败');
    } finally {
      setDirPickerCreatingFolder(false);
    }
  }, [
    apiClient,
    dirPickerNewFolderName,
    dirPickerPath,
    dirPickerTarget,
    loadDirEntries,
    onProjectRootChange,
    onTerminalRootChange,
  ]);

  const chooseDir = useCallback((path: string | null) => {
    if (!path) return;
    if (dirPickerTarget === 'project') {
      onProjectRootChange(path);
    } else {
      onTerminalRootChange(path);
    }
    closeDirPicker();
  }, [closeDirPicker, dirPickerTarget, onProjectRootChange, onTerminalRootChange]);

  const loadKeyFileEntries = useCallback(async (path?: string | null) => {
    setKeyFilePickerLoading(true);
    setKeyFilePickerError(null);
    try {
      const data = await apiClient.listFsEntries(path || undefined);
      setKeyFilePickerPath(data?.path ?? null);
      setKeyFilePickerParent(data?.parent ?? null);
      setKeyFilePickerEntries(
        Array.isArray(data?.entries)
          ? data.entries.map((entry: any) => normalizeFsEntry(entry, false))
          : [],
      );
      setKeyFilePickerRoots(
        Array.isArray(data?.roots)
          ? data.roots.map((entry: any) => normalizeFsEntry(entry, false))
          : [],
      );
    } catch (err: any) {
      setKeyFilePickerError(err?.message || '加载文件列表失败');
    } finally {
      setKeyFilePickerLoading(false);
    }
  }, [apiClient]);

  const openKeyFilePicker = useCallback(async (target: KeyFilePickerTarget) => {
    setKeyFilePickerTarget(target);
    setKeyFilePickerError(null);
    setKeyFilePickerOpen(true);
    const currentPath = target === 'private_key'
      ? remotePrivateKeyPath
      : target === 'certificate'
        ? remoteCertificatePath
        : remoteJumpPrivateKeyPath;
    const parentPath = currentPath ? deriveParentPath(currentPath) : null;
    await loadKeyFileEntries(parentPath);
  }, [
    loadKeyFileEntries,
    remoteCertificatePath,
    remoteJumpPrivateKeyPath,
    remotePrivateKeyPath,
  ]);

  const closeKeyFilePicker = useCallback(() => {
    setKeyFilePickerOpen(false);
    setKeyFilePickerError(null);
  }, []);

  const applySelectedKeyFile = useCallback((path: string) => {
    if (!path) return;
    if (keyFilePickerTarget === 'private_key') {
      onRemotePrivateKeyPathChange(path);
    } else if (keyFilePickerTarget === 'certificate') {
      onRemoteCertificatePathChange(path);
    } else {
      onRemoteJumpPrivateKeyPathChange(path);
    }
    closeKeyFilePicker();
  }, [
    closeKeyFilePicker,
    keyFilePickerTarget,
    onRemoteCertificatePathChange,
    onRemoteJumpPrivateKeyPathChange,
    onRemotePrivateKeyPathChange,
  ]);

  const dirPickerItems = useMemo(() => (
    (dirPickerPath ? dirPickerEntries : dirPickerRoots)
      .filter((entry) => showHiddenDirs || !entry.name.startsWith('.'))
  ), [dirPickerEntries, dirPickerPath, dirPickerRoots, showHiddenDirs]);

  const keyFilePickerItems = useMemo(
    () => (keyFilePickerPath ? keyFilePickerEntries : keyFilePickerRoots),
    [keyFilePickerEntries, keyFilePickerPath, keyFilePickerRoots],
  );

  const keyFilePickerTitle = useMemo(
    () => getKeyFilePickerTitle(keyFilePickerTarget),
    [keyFilePickerTarget],
  );

  return {
    keyFilePickerOpen,
    keyFilePickerTitle,
    keyFilePickerPath,
    keyFilePickerParent,
    keyFilePickerLoading,
    keyFilePickerItems,
    keyFilePickerError,
    dirPickerOpen,
    dirPickerTarget,
    dirPickerPath,
    dirPickerParent,
    dirPickerLoading,
    dirPickerItems,
    dirPickerError,
    showHiddenDirs,
    dirPickerCreateModalOpen,
    dirPickerNewFolderName,
    dirPickerCreatingFolder,
    setShowHiddenDirs,
    setDirPickerCreateModalOpen,
    setDirPickerNewFolderName,
    openDirPicker,
    closeDirPicker,
    openCreateDirModal,
    createDirInPicker,
    chooseDir,
    openKeyFilePicker,
    closeKeyFilePicker,
    applySelectedKeyFile,
    loadDirEntries,
    loadKeyFileEntries,
    setKeyFilePickerOpen,
  };
};

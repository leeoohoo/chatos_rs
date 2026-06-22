import { useCallback, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import { deriveParentPath } from '../../lib/domain/filesystem';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  FsEntriesResponse,
  FsMutationResponse,
} from '../../lib/api/client/types';
import type { FsEntry } from '../../types';
import {
  getKeyFilePickerTitle,
  normalizeFsEntry,
  translateSessionListMessage,
  type DirPickerTarget,
  type KeyFilePickerTarget,
} from './helpers';

interface FsPickerApiClient {
  listFsDirectories: (path?: string) => Promise<FsEntriesResponse>;
  createFsDirectory: (basePath: string, name: string) => Promise<FsMutationResponse>;
  listFsEntries: (path?: string) => Promise<FsEntriesResponse>;
}

interface UseLocalFsPickersOptions {
  apiClient: FsPickerApiClient;
  t?: TranslateFn;
  projectRoot: string;
  terminalRoot: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpCertificatePath: string;
  onProjectRootChange: (path: string) => void;
  onTerminalRootChange: (path: string) => void;
  onRemotePrivateKeyPathChange: (path: string) => void;
  onRemoteCertificatePathChange: (path: string) => void;
  onRemoteJumpPrivateKeyPathChange: (path: string) => void;
  onRemoteJumpCertificatePathChange: (path: string) => void;
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
  dirPickerWritable: boolean;
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

const readErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

export const useLocalFsPickers = ({
  apiClient,
  t,
  projectRoot,
  terminalRoot,
  remotePrivateKeyPath,
  remoteCertificatePath,
  remoteJumpPrivateKeyPath,
  remoteJumpCertificatePath,
  onProjectRootChange,
  onTerminalRootChange,
  onRemotePrivateKeyPathChange,
  onRemoteCertificatePathChange,
  onRemoteJumpPrivateKeyPathChange,
  onRemoteJumpCertificatePathChange,
}: UseLocalFsPickersOptions): UseLocalFsPickersResult => {
  const tr = useCallback((key: string) => translateSessionListMessage(t, key), [t]);

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
  const [dirPickerWritable, setDirPickerWritable] = useState(false);
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
      const nextPath = data?.path ?? null;
      const parentFromApi = data?.parent ?? null;
      setDirPickerPath(nextPath);
      setDirPickerParent(parentFromApi || (nextPath ? deriveParentPath(nextPath) : null));
      setDirPickerWritable(Boolean(data?.writable));
      setDirPickerEntries(
        Array.isArray(data?.entries)
          ? data.entries.map((entry) => normalizeFsEntry(entry, true))
          : [],
      );
      setDirPickerRoots(
        Array.isArray(data?.roots)
          ? data.roots.map((entry) => normalizeFsEntry(entry, true))
          : [],
      );
    } catch (err) {
      setDirPickerError(readErrorMessage(err, tr('sessionList.picker.error.loadDirectoriesFailed')));
    } finally {
      setDirPickerLoading(false);
    }
  }, [apiClient, tr]);

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
      setDirPickerError(tr('sessionList.picker.error.enterParentBeforeCreate'));
      return;
    }
    if (!dirPickerWritable) {
      setDirPickerError(tr('sessionList.picker.error.directoryReadonly'));
      return;
    }
    setDirPickerError(null);
    setDirPickerNewFolderName('');
    setDirPickerCreateModalOpen(true);
  }, [dirPickerPath, dirPickerWritable, tr]);

  const createDirInPicker = useCallback(async () => {
    const basePath = dirPickerPath;
    if (!basePath) {
      setDirPickerError(tr('sessionList.picker.error.enterParentBeforeCreate'));
      return;
    }
    if (!dirPickerWritable) {
      setDirPickerError(tr('sessionList.picker.error.directoryReadonly'));
      return;
    }
    const name = dirPickerNewFolderName.trim();
    if (!name) {
      setDirPickerError(tr('sessionList.picker.error.directoryNameRequired'));
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
    } catch (err) {
      setDirPickerError(readErrorMessage(err, tr('sessionList.picker.error.createDirectoryFailed')));
    } finally {
      setDirPickerCreatingFolder(false);
    }
  }, [
    apiClient,
    dirPickerNewFolderName,
    dirPickerPath,
    dirPickerTarget,
    dirPickerWritable,
    loadDirEntries,
    onProjectRootChange,
    onTerminalRootChange,
    tr,
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
          ? data.entries.map((entry) => normalizeFsEntry(entry, false))
          : [],
      );
      setKeyFilePickerRoots(
        Array.isArray(data?.roots)
          ? data.roots.map((entry) => normalizeFsEntry(entry, false))
          : [],
      );
    } catch (err) {
      setKeyFilePickerError(readErrorMessage(err, tr('sessionList.picker.error.loadFilesFailed')));
    } finally {
      setKeyFilePickerLoading(false);
    }
  }, [apiClient, tr]);

  const openKeyFilePicker = useCallback(async (target: KeyFilePickerTarget) => {
    setKeyFilePickerTarget(target);
    setKeyFilePickerError(null);
    setKeyFilePickerOpen(true);
    const currentPath = target === 'private_key'
      ? remotePrivateKeyPath
      : target === 'certificate'
        ? remoteCertificatePath
        : target === 'jump_private_key'
          ? remoteJumpPrivateKeyPath
          : remoteJumpCertificatePath;
    const parentPath = currentPath ? deriveParentPath(currentPath) : null;
    await loadKeyFileEntries(parentPath);
  }, [
    loadKeyFileEntries,
    remoteCertificatePath,
    remoteJumpCertificatePath,
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
    } else if (keyFilePickerTarget === 'jump_private_key') {
      onRemoteJumpPrivateKeyPathChange(path);
    } else {
      onRemoteJumpCertificatePathChange(path);
    }
    closeKeyFilePicker();
  }, [
    closeKeyFilePicker,
    keyFilePickerTarget,
    onRemoteCertificatePathChange,
    onRemoteJumpCertificatePathChange,
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
    () => getKeyFilePickerTitle(keyFilePickerTarget, t),
    [keyFilePickerTarget, t],
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
    dirPickerWritable,
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

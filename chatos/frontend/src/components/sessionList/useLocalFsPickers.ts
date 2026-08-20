// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useMemo, useState, type Dispatch, type SetStateAction } from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { FsEntriesResponse } from '../../lib/api/client/types';
import { deriveParentPath } from '../../lib/domain/filesystem';
import type { FsEntry } from '../../types';
import {
  getKeyFilePickerTitle,
  normalizeFsEntry,
  translateSessionListMessage,
  type KeyFilePickerTarget,
} from './helpers';

interface FsPickerApiClient {
  listFsEntries: (path?: string) => Promise<FsEntriesResponse>;
}

interface UseLocalFsPickersOptions {
  apiClient: FsPickerApiClient;
  t?: TranslateFn;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpCertificatePath: string;
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
  openKeyFilePicker: (target: KeyFilePickerTarget) => Promise<void>;
  closeKeyFilePicker: () => void;
  applySelectedKeyFile: (path: string) => void;
  loadKeyFileEntries: (path?: string | null) => Promise<void>;
  setKeyFilePickerOpen: Dispatch<SetStateAction<boolean>>;
}

const readErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

export const useLocalFsPickers = ({
  apiClient,
  t,
  remotePrivateKeyPath,
  remoteCertificatePath,
  remoteJumpPrivateKeyPath,
  remoteJumpCertificatePath,
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
    } catch (error) {
      setKeyFilePickerError(readErrorMessage(error, tr('sessionList.picker.error.loadFilesFailed')));
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
    await loadKeyFileEntries(currentPath ? deriveParentPath(currentPath) : null);
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
    openKeyFilePicker,
    closeKeyFilePicker,
    applySelectedKeyFile,
    loadKeyFileEntries,
    setKeyFilePickerOpen,
  };
};

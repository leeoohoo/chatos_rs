import { useCallback, useEffect, useRef, useState } from 'react';

import { resolveRemoteSftpErrorMessage } from '../../lib/api/remoteConnectionErrors';
import type { FsEntry } from '../../types';

import {
  normalizeFsEntriesResponse,
  normalizeRemoteEntriesResponse,
  type RemoteSftpClient,
} from './helpers';
import type { RemoteEntry } from './types';

interface UseRemoteSftpBrowsersOptions {
  client: RemoteSftpClient;
  currentRemoteConnectionId: string | null;
  currentRemoteDefaultPath: string;
  setError: (message: string | null) => void;
  getVerificationCode: () => string | null;
  onSecondFactorRequired: (
    error: unknown,
    retryWithCode: (code: string) => Promise<void>,
  ) => boolean;
}

export const useRemoteSftpBrowsers = ({
  client,
  currentRemoteConnectionId,
  currentRemoteDefaultPath,
  setError,
  getVerificationCode,
  onSecondFactorRequired,
}: UseRemoteSftpBrowsersOptions) => {
  const [localPath, setLocalPath] = useState<string | null>(null);
  const [localParent, setLocalParent] = useState<string | null>(null);
  const [localEntries, setLocalEntries] = useState<FsEntry[]>([]);
  const [localRoots, setLocalRoots] = useState<FsEntry[]>([]);
  const [loadingLocal, setLoadingLocal] = useState(false);
  const [selectedLocal, setSelectedLocal] = useState<FsEntry | null>(null);

  const [remotePath, setRemotePath] = useState<string>('.');
  const [remoteParent, setRemoteParent] = useState<string | null>(null);
  const [remoteEntries, setRemoteEntries] = useState<RemoteEntry[]>([]);
  const [loadingRemote, setLoadingRemote] = useState(false);
  const [selectedRemote, setSelectedRemote] = useState<RemoteEntry | null>(null);

  const remotePathRef = useRef<string>('.');
  const localPathRef = useRef<string | null>(null);

  const loadLocal = useCallback(async (path?: string | null) => {
    setLoadingLocal(true);
    setError(null);
    try {
      const normalized = normalizeFsEntriesResponse(await client.listFsEntries(path || undefined));
      setLocalPath(normalized.path);
      setLocalParent(normalized.parent);
      setLocalEntries(normalized.entries);
      setLocalRoots(normalized.roots);
    } catch (error) {
      setError(resolveRemoteSftpErrorMessage(error, '读取本地目录失败'));
    } finally {
      setLoadingLocal(false);
    }
  }, [client, setError]);

  const loadRemote = useCallback(async (path?: string, verificationCodeOverride?: string) => {
    if (!currentRemoteConnectionId) return;
    setLoadingRemote(true);
    setError(null);
    try {
      const verificationCode = (verificationCodeOverride ?? getVerificationCode() ?? '').trim();
      const normalized = normalizeRemoteEntriesResponse(
        await client.listRemoteSftpEntries(
          currentRemoteConnectionId,
          path,
          verificationCode || undefined,
        ),
      );
      setRemotePath(normalized.path);
      setRemoteParent(normalized.parent);
      setRemoteEntries(normalized.entries);
    } catch (error) {
      if ((verificationCodeOverride || '').trim()) {
        throw error;
      }
      if (onSecondFactorRequired(error, async (code) => {
        await loadRemote(path, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(error, '读取远端目录失败'));
    } finally {
      setLoadingRemote(false);
    }
  }, [
    client,
    currentRemoteConnectionId,
    getVerificationCode,
    onSecondFactorRequired,
    setError,
  ]);

  useEffect(() => {
    remotePathRef.current = remotePath;
  }, [remotePath]);

  useEffect(() => {
    localPathRef.current = localPath;
  }, [localPath]);

  useEffect(() => {
    if (!currentRemoteConnectionId) return;
    setSelectedLocal(null);
    setSelectedRemote(null);
    void loadLocal(null);
    void loadRemote(currentRemoteDefaultPath);
  }, [currentRemoteConnectionId, currentRemoteDefaultPath, loadLocal, loadRemote]);

  return {
    localPath,
    localParent,
    localEntries,
    localRoots,
    loadingLocal,
    selectedLocal,
    setSelectedLocal,
    remotePath,
    remoteParent,
    remoteEntries,
    loadingRemote,
    selectedRemote,
    setSelectedRemote,
    loadLocal,
    loadRemote,
    remotePathRef,
    localPathRef,
  };
};

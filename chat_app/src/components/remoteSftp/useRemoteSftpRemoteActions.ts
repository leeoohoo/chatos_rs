import { useCallback, useState } from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import { resolveRemoteSftpErrorMessage } from '../../lib/api/remoteConnectionErrors';
import type { RemoteConnection } from '../../types';
import type { DialogConfirmOptions, DialogPromptOptions } from '../ui/DialogProvider';

import type { RemoteSftpClient } from './helpers';
import {
  joinRemotePath,
  remoteDirname,
} from './helpers';
import type { RemoteEntry } from './types';

interface UseRemoteSftpRemoteActionsOptions {
  client: RemoteSftpClient;
  currentRemoteConnection: RemoteConnection | null;
  remotePath: string;
  selectedRemote: RemoteEntry | null;
  setSelectedRemote: (entry: RemoteEntry | null) => void;
  loadRemote: (path?: string, verificationCodeOverride?: string) => Promise<void>;
  activeVerificationCode: string | null;
  setMessage: (message: string | null) => void;
  setError: (message: string | null) => void;
  prompt: (options: DialogPromptOptions) => Promise<string | null>;
  confirm: (options: DialogConfirmOptions) => Promise<boolean>;
  t: TranslateFn;
  onSecondFactorRequired: (
    error: unknown,
    retryWithCode: (code: string) => Promise<void>,
  ) => boolean;
}

const validateRemoteEntryName = (name: string, emptyMessage: string, invalidMessage: string): string | null => {
  if (!name) {
    return emptyMessage;
  }
  if (name === '.' || name === '..' || /[\\/]/.test(name)) {
    return invalidMessage;
  }
  return null;
};

export const useRemoteSftpRemoteActions = ({
  client,
  currentRemoteConnection,
  remotePath,
  selectedRemote,
  setSelectedRemote,
  loadRemote,
  activeVerificationCode,
  setMessage,
  setError,
  prompt,
  confirm,
  t,
  onSecondFactorRequired,
}: UseRemoteSftpRemoteActionsOptions) => {
  const [remoteActionLoading, setRemoteActionLoading] = useState(false);

  const handleCreateRemoteDirectory = useCallback(async () => {
    if (!currentRemoteConnection) return;
    const name = await prompt({
      title: t('remote.sftp.prompt.createRemoteDirTitle'),
      message: t('remote.sftp.prompt.createRemoteDirMessage'),
      inputLabel: t('remote.sftp.prompt.createRemoteDirInput'),
      placeholder: t('remote.sftp.prompt.createRemoteDirPlaceholder'),
      defaultValue: '',
      confirmText: t('applications.form.submitCreate'),
      cancelText: t('common.cancel'),
      type: 'info',
    });
    if (name === null) return;
    const trimmedName = name.trim();
    const validationError = validateRemoteEntryName(trimmedName, t('remote.sftp.error.dirNameRequired'), t('remote.sftp.error.dirNameInvalid'));
    if (validationError) {
      setError(validationError);
      return;
    }

    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.createRemoteSftpDirectory(
        currentRemoteConnection.id,
        remotePath,
        trimmedName,
        activeVerificationCode || undefined,
      );
      setMessage(t('remote.sftp.success.createdDir', { name: trimmedName }));
      await loadRemote(remotePath);
    } catch (err) {
      if (onSecondFactorRequired(err, async (code) => {
        await client.createRemoteSftpDirectory(
          currentRemoteConnection.id,
          remotePath,
          trimmedName,
          code,
        );
        setMessage(t('remote.sftp.success.createdDir', { name: trimmedName }));
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, t('remote.sftp.error.createDir')));
    } finally {
      setRemoteActionLoading(false);
    }
  }, [
    activeVerificationCode,
    client,
    currentRemoteConnection,
    loadRemote,
    onSecondFactorRequired,
    prompt,
    remotePath,
    setError,
    setMessage,
    t,
  ]);

  const handleRenameRemoteEntry = useCallback(async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError(t('remote.sftp.error.selectRemoteEntry'));
      return;
    }
    const nextName = await prompt({
      title: t('remote.sftp.prompt.renameTitle'),
      message: t('remote.sftp.prompt.renameMessage', { name: selectedRemote.name }),
      inputLabel: t('remote.sftp.prompt.renameInput'),
      defaultValue: selectedRemote.name,
      confirmText: t('remote.sftp.browser.rename'),
      cancelText: t('common.cancel'),
      type: 'info',
    });
    if (nextName === null) return;
    const trimmedName = nextName.trim();
    const validationError = validateRemoteEntryName(trimmedName, t('remote.sftp.error.newNameRequired'), t('remote.sftp.error.newNameInvalid'));
    if (validationError) {
      setError(validationError);
      return;
    }
    if (trimmedName === selectedRemote.name) {
      return;
    }

    const targetPath = joinRemotePath(remoteDirname(selectedRemote.path), trimmedName);
    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.renameRemoteSftpEntry(
        currentRemoteConnection.id,
        selectedRemote.path,
        targetPath,
        activeVerificationCode || undefined,
      );
      setMessage(t('remote.sftp.success.renamed', { from: selectedRemote.name, to: trimmedName }));
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (err) {
      if (onSecondFactorRequired(err, async (code) => {
        await client.renameRemoteSftpEntry(
          currentRemoteConnection.id,
          selectedRemote.path,
          targetPath,
          code,
        );
        setMessage(t('remote.sftp.success.renamed', { from: selectedRemote.name, to: trimmedName }));
        setSelectedRemote(null);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, t('remote.sftp.error.rename')));
    } finally {
      setRemoteActionLoading(false);
    }
  }, [
    activeVerificationCode,
    client,
    currentRemoteConnection,
    loadRemote,
    onSecondFactorRequired,
    prompt,
    remotePath,
    selectedRemote,
    setError,
    setMessage,
    setSelectedRemote,
    t,
  ]);

  const handleDeleteRemoteEntry = useCallback(async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError(t('remote.sftp.error.selectRemoteEntry'));
      return;
    }

    const confirmed = await confirm({
      title: selectedRemote.isDir ? t('remote.sftp.confirm.deleteEntryTitleDir') : t('remote.sftp.confirm.deleteEntryTitleFile'),
      message: t('remote.sftp.confirm.deleteEntryMessage', {
        kind: selectedRemote.isDir ? t('remote.sftp.confirm.entryKindDir') : t('remote.sftp.confirm.entryKindFile'),
        name: selectedRemote.name,
      }),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) return;

    let recursive = false;
    if (selectedRemote.isDir) {
      recursive = await confirm({
        title: t('remote.sftp.confirm.deleteModeTitle'),
        message: t('remote.sftp.confirm.deleteModeMessage'),
        description: t('remote.sftp.confirm.deleteModeDescription'),
        confirmText: t('remote.sftp.confirm.deleteModeRecursive'),
        cancelText: t('remote.sftp.confirm.deleteModeEmptyOnly'),
        type: 'warning',
      });
    }

    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.deleteRemoteSftpEntry(
        currentRemoteConnection.id,
        selectedRemote.path,
        recursive,
        activeVerificationCode || undefined,
      );
      setMessage(t('remote.sftp.success.deleted', { name: selectedRemote.name }));
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (err) {
      if (onSecondFactorRequired(err, async (code) => {
        await client.deleteRemoteSftpEntry(
          currentRemoteConnection.id,
          selectedRemote.path,
          recursive,
          code,
        );
        setMessage(t('remote.sftp.success.deleted', { name: selectedRemote.name }));
        setSelectedRemote(null);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, t('remote.sftp.error.delete')));
    } finally {
      setRemoteActionLoading(false);
    }
  }, [
    activeVerificationCode,
    client,
    confirm,
    currentRemoteConnection,
    loadRemote,
    onSecondFactorRequired,
    remotePath,
    selectedRemote,
    setError,
    setMessage,
    setSelectedRemote,
    t,
  ]);

  return {
    remoteActionLoading,
    handleCreateRemoteDirectory,
    handleRenameRemoteEntry,
    handleDeleteRemoteEntry,
  };
};

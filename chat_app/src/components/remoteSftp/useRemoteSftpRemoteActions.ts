import { useCallback, useState } from 'react';

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
  onSecondFactorRequired,
}: UseRemoteSftpRemoteActionsOptions) => {
  const [remoteActionLoading, setRemoteActionLoading] = useState(false);

  const handleCreateRemoteDirectory = useCallback(async () => {
    if (!currentRemoteConnection) return;
    const name = await prompt({
      title: '新建远端目录',
      message: '请输入新目录名称',
      inputLabel: '目录名称',
      placeholder: '例如 logs',
      defaultValue: '',
      confirmText: '创建',
      cancelText: '取消',
      type: 'info',
    });
    if (name === null) return;
    const trimmedName = name.trim();
    const validationError = validateRemoteEntryName(trimmedName, '目录名称不能为空', '目录名称不合法');
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
      setMessage(`已创建目录: ${trimmedName}`);
      await loadRemote(remotePath);
    } catch (err) {
      if (onSecondFactorRequired(err, async (code) => {
        await client.createRemoteSftpDirectory(
          currentRemoteConnection.id,
          remotePath,
          trimmedName,
          code,
        );
        setMessage(`已创建目录: ${trimmedName}`);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, '创建目录失败'));
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
  ]);

  const handleRenameRemoteEntry = useCallback(async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先选择远端文件或目录');
      return;
    }
    const nextName = await prompt({
      title: '重命名远端条目',
      message: `请输入 "${selectedRemote.name}" 的新名称`,
      inputLabel: '新名称',
      defaultValue: selectedRemote.name,
      confirmText: '重命名',
      cancelText: '取消',
      type: 'info',
    });
    if (nextName === null) return;
    const trimmedName = nextName.trim();
    const validationError = validateRemoteEntryName(trimmedName, '新名称不能为空', '新名称不合法');
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
      setMessage(`已重命名: ${selectedRemote.name} → ${trimmedName}`);
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
        setMessage(`已重命名: ${selectedRemote.name} → ${trimmedName}`);
        setSelectedRemote(null);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, '重命名失败'));
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
  ]);

  const handleDeleteRemoteEntry = useCallback(async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先选择远端文件或目录');
      return;
    }

    const confirmed = await confirm({
      title: selectedRemote.isDir ? '删除远端目录' : '删除远端文件',
      message: `确认删除${selectedRemote.isDir ? '目录' : '文件'} "${selectedRemote.name}" 吗？`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
    });
    if (!confirmed) return;

    let recursive = false;
    if (selectedRemote.isDir) {
      recursive = await confirm({
        title: '目录删除方式',
        message: '是否递归删除该目录及其全部内容？',
        description: '选择“仅删除空目录”将只在目录为空时执行删除。',
        confirmText: '递归删除',
        cancelText: '仅删除空目录',
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
      setMessage(`已删除: ${selectedRemote.name}`);
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
        setMessage(`已删除: ${selectedRemote.name}`);
        setSelectedRemote(null);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, '删除失败'));
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
  ]);

  return {
    remoteActionLoading,
    handleCreateRemoteDirectory,
    handleRenameRemoteEntry,
    handleDeleteRemoteEntry,
  };
};

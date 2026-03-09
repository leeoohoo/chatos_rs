import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useChatApiClientFromContext, useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';
import { cn } from '../lib/utils';
import type { FsEntry } from '../types';

interface RemoteSftpPanelProps {
  className?: string;
}

interface RemoteEntry {
  name: string;
  path: string;
  isDir: boolean;
  size?: number | null;
  modifiedAt?: string | null;
}

interface SftpTransferStatus {
  id: string;
  direction: 'upload' | 'download';
  state: 'pending' | 'running' | 'cancelling' | 'success' | 'error' | 'cancelled';
  totalBytes: number | null;
  transferredBytes: number;
  percent: number | null;
  currentPath: string | null;
  message: string | null;
  error: string | null;
}

interface SftpTransferRequest {
  id: string;
  direction: 'upload' | 'download';
  localSource: string;
  remoteSource: string;
  fallbackSuccess: string;
  label: string;
}

const normalizeLocalEntry = (raw: any): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

const normalizeRemoteEntry = (raw: any): RemoteEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

const normalizeTransferStatus = (raw: any): SftpTransferStatus => ({
  id: raw?.id ?? '',
  direction: (raw?.direction ?? 'upload') as 'upload' | 'download',
  state: (raw?.state ?? 'pending') as 'pending' | 'running' | 'cancelling' | 'success' | 'error' | 'cancelled',
  totalBytes: raw?.total_bytes ?? raw?.totalBytes ?? null,
  transferredBytes: Number(raw?.transferred_bytes ?? raw?.transferredBytes ?? 0),
  percent: typeof raw?.percent === 'number' ? raw.percent : null,
  currentPath: raw?.current_path ?? raw?.currentPath ?? null,
  message: raw?.message ?? null,
  error: raw?.error ?? null,
});

const formatBytes = (value: number): string => {
  if (!Number.isFinite(value) || value <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = value;
  let idx = 0;
  while (size >= 1024 && idx < units.length - 1) {
    size /= 1024;
    idx += 1;
  }
  return `${size.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
};

const joinLocalPath = (base: string, name: string): string => {
  const normalized = base.replace(/[\\/]+$/, '');
  if (!normalized) return name;
  const sep = normalized.includes('\\') ? '\\' : '/';
  return `${normalized}${sep}${name}`;
};

const joinRemotePath = (base: string, name: string): string => {
  const normalized = base.replace(/\/+$/, '');
  if (!normalized || normalized === '.') return name;
  if (normalized === '/') return `/${name}`;
  return `${normalized}/${name}`;
};

const remoteDirname = (path: string): string => {
  const normalized = path.trim().replace(/\/+$/, '');
  if (!normalized || normalized === '.' || normalized === '/') return '.';
  const idx = normalized.lastIndexOf('/');
  if (idx < 0) return '.';
  if (idx === 0) return '/';
  return normalized.slice(0, idx);
};

const SFTP_ERROR_CODE_MESSAGES: Record<string, string> = {
  invalid_argument: '请求参数不合法',
  invalid_path: '路径不存在或不可访问',
  invalid_directory_name: '目录名称不合法',
  transfer_not_found: '传输任务不存在',
  transfer_not_active: '传输任务不存在或已结束',
  transfer_cancelled: '传输已取消',
  timeout: '操作超时，请稍后重试',
  local_io_error: '本地文件读写失败',
  remote_auth_failed: '远端认证失败',
  remote_path_not_found: '远端路径不存在',
  remote_permission_denied: '远端权限不足',
  remote_network_disconnected: '远端网络连接中断',
  remote_error: '远端连接或协议错误',
};

const resolveSftpErrorMessage = (error: any, fallback: string): string => {
  const code = typeof error?.code === 'string' ? error.code : '';
  const mapped = SFTP_ERROR_CODE_MESSAGES[code];
  const raw = typeof error?.message === 'string' && error.message.trim().length > 0
    ? error.message.trim()
    : fallback;

  if (!mapped) {
    return raw;
  }

  if (
    code === 'remote_error' ||
    code === 'local_io_error' ||
    code === 'remote_auth_failed' ||
    code === 'remote_network_disconnected'
  ) {
    return `${mapped}: ${raw}`;
  }

  return mapped;
};

const RemoteSftpPanel: React.FC<RemoteSftpPanelProps> = ({ className }) => {
  const {
    currentRemoteConnection,
    selectRemoteConnection,
  } = useChatStoreFromContext();
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const currentRemoteConnectionId = currentRemoteConnection?.id ?? null;
  const currentRemoteDefaultPath = currentRemoteConnection?.defaultRemotePath || '.';

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

  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [transfering, setTransfering] = useState(false);
  const [transferStatus, setTransferStatus] = useState<SftpTransferStatus | null>(null);
  const [queuedTransfers, setQueuedTransfers] = useState<SftpTransferRequest[]>([]);
  const [remoteActionLoading, setRemoteActionLoading] = useState(false);
  const transferPollTimerRef = useRef<number | null>(null);
  const transferPollingBusyRef = useRef(false);
  const remotePathRef = useRef<string>('.');
  const localPathRef = useRef<string | null>(null);
  const transferQueueSeqRef = useRef(0);

  const loadLocal = useCallback(async (path?: string | null) => {
    setLoadingLocal(true);
    setError(null);
    try {
      const data = await client.listFsEntries(path || undefined);
      const entries = Array.isArray(data?.entries) ? data.entries.map(normalizeLocalEntry) : [];
      const roots = Array.isArray(data?.roots) ? data.roots.map(normalizeLocalEntry) : [];
      setLocalPath(data?.path ?? null);
      setLocalParent(data?.parent ?? null);
      setLocalEntries(entries);
      setLocalRoots(roots);
    } catch (err: any) {
      setError(resolveSftpErrorMessage(err, '读取本地目录失败'));
    } finally {
      setLoadingLocal(false);
    }
  }, [client]);

  const loadRemote = useCallback(async (path?: string) => {
    if (!currentRemoteConnectionId) return;
    setLoadingRemote(true);
    setError(null);
    try {
      const data = await client.listRemoteSftpEntries(currentRemoteConnectionId, path);
      const entries = Array.isArray(data?.entries) ? data.entries.map(normalizeRemoteEntry) : [];
      setRemotePath(data?.path ?? '.');
      setRemoteParent(data?.parent ?? null);
      setRemoteEntries(entries);
    } catch (err: any) {
      setError(resolveSftpErrorMessage(err, '读取远端目录失败'));
    } finally {
      setLoadingRemote(false);
    }
  }, [client, currentRemoteConnectionId]);

  useEffect(() => {
    remotePathRef.current = remotePath;
  }, [remotePath]);

  useEffect(() => {
    localPathRef.current = localPath;
  }, [localPath]);

  const stopTransferPolling = useCallback(() => {
    if (transferPollTimerRef.current !== null) {
      window.clearInterval(transferPollTimerRef.current);
      transferPollTimerRef.current = null;
    }
    transferPollingBusyRef.current = false;
  }, []);

  useEffect(() => {
    if (!currentRemoteConnectionId) return;
    stopTransferPolling();
    setSelectedLocal(null);
    setSelectedRemote(null);
    setMessage(null);
    setError(null);
    setTransferStatus(null);
    setTransfering(false);
    setQueuedTransfers([]);
    void loadLocal(null);
    void loadRemote(currentRemoteDefaultPath);
  }, [currentRemoteConnectionId, currentRemoteDefaultPath, loadLocal, loadRemote, stopTransferPolling]);

  useEffect(() => () => stopTransferPolling(), [stopTransferPolling]);

  const startTransfer = useCallback(async (
    direction: 'upload' | 'download',
    localSource: string,
    remoteSource: string,
    fallbackSuccess: string,
  ) => {
    if (!currentRemoteConnectionId) return;

    stopTransferPolling();
    setTransfering(true);
    setTransferStatus(null);
    setError(null);
    setMessage(null);

    try {
      const startedRaw = await client.startRemoteSftpTransfer(currentRemoteConnectionId, {
        direction,
        local_path: localSource,
        remote_path: remoteSource,
      });
      const started = normalizeTransferStatus(startedRaw);
      setTransferStatus(started);

      transferPollTimerRef.current = window.setInterval(async () => {
        if (transferPollingBusyRef.current) return;
        transferPollingBusyRef.current = true;
        try {
          const latestRaw = await client.getRemoteSftpTransferStatus(currentRemoteConnectionId, started.id);
          const latest = normalizeTransferStatus(latestRaw);
          setTransferStatus(latest);
          if (latest.state === 'success') {
            stopTransferPolling();
            setTransfering(false);
            setMessage(latest.message || fallbackSuccess);
            await loadRemote(remotePathRef.current);
            if (localPathRef.current !== null) {
              await loadLocal(localPathRef.current);
            } else {
              await loadLocal(null);
            }
          } else if (latest.state === 'cancelled') {
            stopTransferPolling();
            setTransfering(false);
            setMessage(latest.message || '传输已取消');
          } else if (latest.state === 'error') {
            stopTransferPolling();
            setTransfering(false);
            setError(latest.error || '传输失败');
          }
        } catch (err: any) {
          stopTransferPolling();
          setTransfering(false);
          setError(resolveSftpErrorMessage(err, '查询传输进度失败'));
        } finally {
          transferPollingBusyRef.current = false;
        }
      }, 350);
    } catch (err: any) {
      setTransfering(false);
      setTransferStatus(null);
      setError(resolveSftpErrorMessage(err, '启动传输失败'));
    }
  }, [client, currentRemoteConnectionId, loadLocal, loadRemote, stopTransferPolling]);

  const enqueueTransfer = useCallback((request: Omit<SftpTransferRequest, 'id'>) => {
    const queuedRequest: SftpTransferRequest = {
      ...request,
      id: `transfer-${Date.now()}-${transferQueueSeqRef.current}`,
    };
    transferQueueSeqRef.current += 1;

    setQueuedTransfers((prev) => {
      const next = [...prev, queuedRequest];
      if (prev.length > 0 || transfering) {
        setMessage(`已加入队列：${queuedRequest.label}（当前队列 ${next.length}）`);
        setError(null);
      }
      return next;
    });
  }, [transfering]);

  useEffect(() => {
    if (!currentRemoteConnectionId || transfering || queuedTransfers.length === 0) return;
    const [next, ...rest] = queuedTransfers;
    setQueuedTransfers(rest);
    setMessage(`开始队列任务：${next.label}${rest.length > 0 ? `（剩余 ${rest.length}）` : ''}`);
    setError(null);
    void startTransfer(next.direction, next.localSource, next.remoteSource, next.fallbackSuccess);
  }, [currentRemoteConnectionId, queuedTransfers, transfering, startTransfer]);

  const handleRemoveQueuedTransfer = useCallback((transferId: string) => {
    setQueuedTransfers((prev) => prev.filter((item) => item.id !== transferId));
  }, []);

  const handleClearQueuedTransfers = useCallback(() => {
    if (queuedTransfers.length === 0) return;
    setQueuedTransfers([]);
    setMessage('已清空传输队列');
    setError(null);
  }, [queuedTransfers.length]);

  const handleCancelTransfer = useCallback(async () => {
    if (!currentRemoteConnectionId || !transferStatus?.id || !transfering) return;
    try {
      const statusRaw = await client.cancelRemoteSftpTransfer(currentRemoteConnectionId, transferStatus.id);
      const status = normalizeTransferStatus(statusRaw);
      setTransferStatus(status);
      setMessage(null);
      setError(null);
    } catch (err: any) {
      setError(resolveSftpErrorMessage(err, '取消传输失败'));
    }
  }, [client, currentRemoteConnectionId, transferStatus?.id, transfering]);

  const handleUpload = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedLocal) {
      setError('请先在右侧选择一个本地文件或目录');
      return;
    }
    const target = joinRemotePath(remotePath, selectedLocal.name);
    enqueueTransfer({
      direction: 'upload',
      localSource: selectedLocal.path,
      remoteSource: target,
      fallbackSuccess: `上传成功: ${selectedLocal.name}`,
      label: `上传 ${selectedLocal.name}`,
    });
  };

  const handleDownload = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先在左侧选择一个远端文件或目录');
      return;
    }
    if (!localPath) {
      setError('请先在右侧进入一个本地目录');
      return;
    }
    const target = joinLocalPath(localPath, selectedRemote.name);
    enqueueTransfer({
      direction: 'download',
      localSource: target,
      remoteSource: selectedRemote.path,
      fallbackSuccess: `下载成功: ${selectedRemote.name}`,
      label: `下载 ${selectedRemote.name}`,
    });
  };

  const handleCreateRemoteDirectory = async () => {
    if (!currentRemoteConnection) return;
    const name = window.prompt('请输入新目录名称');
    if (name === null) return;
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError('目录名称不能为空');
      return;
    }
    if (trimmedName === '.' || trimmedName === '..' || /[\\/]/.test(trimmedName)) {
      setError('目录名称不合法');
      return;
    }

    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.createRemoteSftpDirectory(currentRemoteConnection.id, remotePath, trimmedName);
      setMessage(`已创建目录: ${trimmedName}`);
      await loadRemote(remotePath);
    } catch (err: any) {
      setError(resolveSftpErrorMessage(err, '创建目录失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  const handleRenameRemoteEntry = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先选择远端文件或目录');
      return;
    }
    const nextName = window.prompt('请输入新名称', selectedRemote.name);
    if (nextName === null) return;
    const trimmedName = nextName.trim();
    if (!trimmedName) {
      setError('新名称不能为空');
      return;
    }
    if (trimmedName === '.' || trimmedName === '..' || /[\\/]/.test(trimmedName)) {
      setError('新名称不合法');
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
      await client.renameRemoteSftpEntry(currentRemoteConnection.id, selectedRemote.path, targetPath);
      setMessage(`已重命名: ${selectedRemote.name} → ${trimmedName}`);
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (err: any) {
      setError(resolveSftpErrorMessage(err, '重命名失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  const handleDeleteRemoteEntry = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先选择远端文件或目录');
      return;
    }

    const confirmed = window.confirm(`确认删除 ${selectedRemote.isDir ? '目录' : '文件'} "${selectedRemote.name}" 吗？`);
    if (!confirmed) return;

    let recursive = false;
    if (selectedRemote.isDir) {
      recursive = window.confirm('是否递归删除该目录及其全部内容？\n选择“取消”将仅删除空目录。');
    }

    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.deleteRemoteSftpEntry(currentRemoteConnection.id, selectedRemote.path, recursive);
      setMessage(`已删除: ${selectedRemote.name}`);
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (err: any) {
      setError(resolveSftpErrorMessage(err, '删除失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  if (!currentRemoteConnection) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        请选择一个远端连接
      </div>
    );
  }

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <div className="flex items-center justify-between border-b border-border px-4 py-2 gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground truncate">SFTP · {currentRemoteConnection.name}</div>
          <div className="text-xs text-muted-foreground truncate">
            {currentRemoteConnection.username}@{currentRemoteConnection.host}:{currentRemoteConnection.port}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={handleUpload}
            disabled={remoteActionLoading}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            上传 →
          </button>
          <button
            type="button"
            onClick={handleDownload}
            disabled={remoteActionLoading}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            ← 下载
          </button>
          <button
            type="button"
            onClick={() => void selectRemoteConnection(currentRemoteConnection.id)}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
          >
            返回终端
          </button>
        </div>
      </div>

      {message && <div className="px-4 py-2 text-xs text-emerald-600">{message}</div>}
      {error && <div className="px-4 py-2 text-xs text-destructive">{error}</div>}
      {transferStatus && transfering && (
        <div className="px-4 py-2 border-b border-border bg-muted/30">
          <div className="flex items-center justify-between text-xs text-foreground">
            <span>
              {transferStatus.state === 'cancelling'
                ? '取消中...'
                : transferStatus.direction === 'upload'
                  ? '上传中'
                  : '下载中'}
            </span>
            <div className="flex items-center gap-2">
              <span>
                {transferStatus.percent !== null
                  ? `${transferStatus.percent.toFixed(1)}%`
                  : `${formatBytes(transferStatus.transferredBytes)}${transferStatus.totalBytes !== null ? ` / ${formatBytes(transferStatus.totalBytes)}` : ''}`}
              </span>
              <button
                type="button"
                onClick={() => void handleCancelTransfer()}
                disabled={transferStatus.state === 'cancelling'}
                className="rounded border border-border px-2 py-0.5 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                取消
              </button>
            </div>
          </div>
          <div className="mt-2 h-1.5 rounded bg-muted overflow-hidden">
            {transferStatus.percent !== null ? (
              <div
                className="h-full bg-blue-500 transition-all duration-200"
                style={{ width: `${Math.max(2, Math.min(100, transferStatus.percent))}%` }}
              />
            ) : (
              <div className="h-full w-1/3 bg-blue-500/80 animate-pulse" />
            )}
          </div>
          {transferStatus.currentPath && (
            <div className="mt-1 text-[11px] text-muted-foreground truncate">{transferStatus.currentPath}</div>
          )}
        </div>
      )}
      {queuedTransfers.length > 0 && (
        <div className="px-4 py-2 border-b border-border bg-muted/20">
          <div className="flex items-center justify-between text-[11px] text-muted-foreground">
            <span>排队任务：{queuedTransfers.length}</span>
            <button
              type="button"
              onClick={handleClearQueuedTransfers}
              className="rounded border border-border px-2 py-0.5 text-[11px] text-foreground hover:bg-accent"
            >
              清空队列
            </button>
          </div>
          <div className="mt-1 space-y-1">
            {queuedTransfers.slice(0, 3).map((item, index) => (
              <div
                key={item.id}
                className="flex items-center justify-between gap-2 rounded border border-border px-2 py-1 text-[11px]"
              >
                <span className="truncate text-foreground">{index + 1}. {item.label}</span>
                <button
                  type="button"
                  onClick={() => handleRemoveQueuedTransfer(item.id)}
                  className="rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-accent"
                >
                  移除
                </button>
              </div>
            ))}
            {queuedTransfers.length > 3 && (
              <div className="text-[11px] text-muted-foreground">还有 {queuedTransfers.length - 3} 项待传输…</div>
            )}
          </div>
        </div>
      )}

      <div className="flex flex-1 min-h-0 divide-x divide-border">
        <div className="flex-1 min-w-0 flex flex-col">
          <div className="border-b border-border px-3 py-2 flex items-center justify-between gap-2">
            <div className="text-xs text-muted-foreground truncate">远端：{remotePath}</div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={handleCreateRemoteDirectory}
                disabled={transfering || remoteActionLoading}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                新建目录
              </button>
              <button
                type="button"
                onClick={handleRenameRemoteEntry}
                disabled={!selectedRemote || transfering || remoteActionLoading}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                重命名
              </button>
              <button
                type="button"
                onClick={handleDeleteRemoteEntry}
                disabled={!selectedRemote || transfering || remoteActionLoading}
                className="rounded border border-border px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                删除
              </button>
              <button
                type="button"
                onClick={() => void loadRemote(remoteParent || undefined)}
                disabled={!remoteParent}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                上级
              </button>
              <button
                type="button"
                onClick={() => void loadRemote(remotePath)}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent"
              >
                刷新
              </button>
            </div>
          </div>
          <div className="flex-1 overflow-auto">
            {loadingRemote ? (
              <div className="px-3 py-4 text-xs text-muted-foreground">加载中...</div>
            ) : remoteEntries.length === 0 ? (
              <div className="px-3 py-4 text-xs text-muted-foreground">空目录</div>
            ) : (
              <div className="p-2 space-y-1">
                {remoteEntries.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    onClick={() => {
                      setSelectedRemote(entry);
                    }}
                    onDoubleClick={() => {
                      if (entry.isDir) {
                        setSelectedRemote(null);
                        void loadRemote(entry.path);
                      }
                    }}
                    className={cn(
                      'w-full text-left rounded border px-2 py-1.5 text-xs',
                      selectedRemote?.path === entry.path ? 'border-blue-500 bg-blue-500/10' : 'border-border hover:bg-accent',
                    )}
                  >
                    <div className="truncate text-foreground">
                      {entry.isDir ? '📁' : '📄'} {entry.name}
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="flex-1 min-w-0 flex flex-col">
          <div className="border-b border-border px-3 py-2 flex items-center justify-between gap-2">
            <div className="text-xs text-muted-foreground truncate">本地：{localPath || '请选择根目录'}</div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => void loadLocal(localParent)}
                disabled={!localParent && localPath !== null}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                上级
              </button>
              <button
                type="button"
                onClick={() => void loadLocal(localPath)}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent"
              >
                刷新
              </button>
            </div>
          </div>
          <div className="flex-1 overflow-auto">
            {loadingLocal ? (
              <div className="px-3 py-4 text-xs text-muted-foreground">加载中...</div>
            ) : localPath === null ? (
              <div className="p-2 space-y-1">
                {localRoots.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    onClick={() => void loadLocal(entry.path)}
                    className="w-full text-left rounded border border-border px-2 py-1.5 text-xs hover:bg-accent"
                  >
                    <div className="truncate text-foreground">📁 {entry.name || entry.path}</div>
                  </button>
                ))}
              </div>
            ) : localEntries.length === 0 ? (
              <div className="px-3 py-4 text-xs text-muted-foreground">空目录</div>
            ) : (
              <div className="p-2 space-y-1">
                {localEntries.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    onClick={() => {
                      setSelectedLocal(entry);
                    }}
                    onDoubleClick={() => {
                      if (entry.isDir) {
                        setSelectedLocal(null);
                        void loadLocal(entry.path);
                      }
                    }}
                    className={cn(
                      'w-full text-left rounded border px-2 py-1.5 text-xs',
                      selectedLocal?.path === entry.path ? 'border-blue-500 bg-blue-500/10' : 'border-border hover:bg-accent',
                    )}
                  >
                    <div className="truncate text-foreground">
                      {entry.isDir ? '📁' : '📄'} {entry.name}
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default RemoteSftpPanel;

import type { FC } from 'react';
import type { RemoteConnection } from '../../types';
import { cn } from '../../lib/utils';
import type { ConnectionState } from './types';

interface RemoteTerminalHeaderProps {
  connection: RemoteConnection;
  connectionState: ConnectionState;
  busy: boolean;
  disconnecting: boolean;
  onReconnect: () => void;
  onDisconnect: () => void;
  onOpenSftp: () => void;
}

const formatConnectionState = (connectionState: ConnectionState): string => {
  if (connectionState === 'connected') return '已连接';
  if (connectionState === 'connecting') return '连接中';
  if (connectionState === 'error') return '连接错误';
  return '未连接';
};

export const RemoteTerminalHeader: FC<RemoteTerminalHeaderProps> = ({
  connection,
  connectionState,
  busy,
  disconnecting,
  onReconnect,
  onDisconnect,
  onOpenSftp,
}) => (
  <div className="flex items-center justify-between border-b border-border px-4 py-2 gap-3">
    <div className="min-w-0">
      <div className="text-sm font-medium text-foreground truncate">{connection.name}</div>
      <div className="text-xs text-muted-foreground truncate">
        {connection.username}@{connection.host}:{connection.port}
      </div>
    </div>
    <div className="flex items-center gap-2 text-xs text-muted-foreground shrink-0">
      <span className={cn(
        'inline-flex items-center gap-1',
        connectionState === 'connected' ? 'text-emerald-500' : connectionState === 'error' ? 'text-destructive' : 'text-muted-foreground',
      )}>
        <span className={cn(
          'inline-block h-2 w-2 rounded-full',
          connectionState === 'connected' ? 'bg-emerald-500' : connectionState === 'error' ? 'bg-destructive' : 'bg-muted-foreground/50',
        )} />
        {formatConnectionState(connectionState)}
      </span>
      <span>{busy ? '忙碌' : '空闲'}</span>
      <button
        type="button"
        onClick={onReconnect}
        disabled={disconnecting}
        className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
      >
        重连
      </button>
      <button
        type="button"
        onClick={onDisconnect}
        disabled={disconnecting || connectionState === 'disconnected'}
        className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50"
      >
        {disconnecting ? '断开中...' : '断开'}
      </button>
      <button
        type="button"
        onClick={onOpenSftp}
        className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
      >
        SFTP
      </button>
    </div>
  </div>
);

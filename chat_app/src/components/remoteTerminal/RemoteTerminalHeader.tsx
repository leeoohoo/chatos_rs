import type { FC } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
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

export const RemoteTerminalHeader: FC<RemoteTerminalHeaderProps> = ({
  connection,
  connectionState,
  busy,
  disconnecting,
  onReconnect,
  onDisconnect,
  onOpenSftp,
}) => {
  const { t } = useI18n();
  const formatConnectionState = (state: ConnectionState): string => {
    if (state === 'connected') return t('remote.terminal.header.connected');
    if (state === 'connecting') return t('remote.terminal.header.connecting');
    if (state === 'error') return t('remote.terminal.header.error');
    return t('remote.terminal.header.disconnected');
  };

  return (
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
        <span>{busy ? t('remote.terminal.header.busy') : t('remote.terminal.header.idle')}</span>
        <button
          type="button"
          onClick={onReconnect}
          disabled={disconnecting}
          className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
        >
          {t('remote.terminal.header.reconnect')}
        </button>
        <button
          type="button"
          onClick={onDisconnect}
          disabled={disconnecting || connectionState === 'disconnected'}
          className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50"
        >
          {disconnecting ? t('remote.terminal.header.disconnecting') : t('remote.terminal.header.disconnect')}
        </button>
        <button
          type="button"
          onClick={onOpenSftp}
          className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
        >
          {t('remote.terminal.header.sftp')}
        </button>
      </div>
    </div>
  );
};

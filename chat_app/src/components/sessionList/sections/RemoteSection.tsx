import React from 'react';
import { cn } from '../../../lib/utils';
import type { RemoteConnection } from '../../../types';
import { DotsVerticalIcon, PencilIcon, PlusIcon, TrashIcon } from '../../ui/icons';

interface RemoteSectionProps {
  expanded: boolean;
  remoteConnections: RemoteConnection[];
  currentRemoteConnectionId?: string | null;
  isRefreshing: boolean;
  onToggle: () => void;
  onRefresh: () => void;
  onCreate: () => void;
  onSelect: (connectionId: string) => void;
  onOpenSftp: (connectionId: string) => void;
  onEdit: (connection: RemoteConnection) => void;
  onTest: (connection: RemoteConnection) => void | Promise<void>;
  onDelete: (connectionId: string) => void;
  onToggleActionMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  closeActionMenus: () => void;
  formatTimeAgo: (date: string | Date | undefined | null) => string;
}

export const RemoteSection: React.FC<RemoteSectionProps> = ({
  expanded,
  remoteConnections,
  currentRemoteConnectionId,
  isRefreshing,
  onToggle,
  onRefresh,
  onCreate,
  onSelect,
  onOpenSftp,
  onEdit,
  onTest,
  onDelete,
  onToggleActionMenu,
  closeActionMenus,
  formatTimeAgo,
}) => {
  return (
    <div className={cn('flex flex-col min-h-0', expanded ? 'flex-1' : 'shrink-0')}>
      <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
        <button
          type="button"
          onClick={onToggle}
          className="flex items-center gap-2 uppercase tracking-wide"
        >
          <span>{expanded ? '▾' : '▸'}</span>
          <span>REMOTE</span>
        </button>
        <div className="flex items-center gap-1">
          <button
            onClick={onRefresh}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title="刷新远端连接列表"
          >
            <svg className={cn('w-4 h-4', isRefreshing && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
            </svg>
          </button>
          <button
            type="button"
            onClick={onCreate}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title="新增远端连接"
          >
            <PlusIcon className="w-4 h-4" />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="flex-1 min-h-0 overflow-y-auto">
          {remoteConnections.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">
              还没有远端连接，点击右侧 + 新建。
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {remoteConnections.map((connection) => (
                <div
                  key={connection.id}
                  className={cn(
                    'group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors',
                    currentRemoteConnectionId === connection.id
                      ? 'bg-accent border border-border'
                      : 'hover:bg-accent/50',
                  )}
                  onClick={() => onSelect(connection.id)}
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <h3 className="text-sm font-medium text-foreground truncate">
                        {connection.name}
                      </h3>
                      <span className="inline-flex items-center text-[10px] px-1.5 py-0.5 rounded border border-blue-500/40 text-blue-600">
                        SSH
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground truncate" title={`${connection.username}@${connection.host}:${connection.port}`}>
                      {connection.username}@{connection.host}:{connection.port}
                    </div>
                    {connection.lastActiveAt && (
                      <div className="mt-1 text-[10px] text-muted-foreground/70">
                        最近活动：{formatTimeAgo(connection.lastActiveAt)}
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onOpenSftp(connection.id);
                      }}
                      className="rounded border border-border px-2 py-1 text-[10px] text-foreground hover:bg-accent"
                      title="打开 SFTP"
                    >
                      SFTP
                    </button>
                    <div className="relative" data-action-menu-root="true">
                      <button
                        className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                        onClick={onToggleActionMenu}
                      >
                        <DotsVerticalIcon className="w-4 h-4" />
                      </button>
                      <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-36 bg-popover border border-border rounded-md shadow-lg">
                        <div className="py-1">
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              onEdit(connection);
                              closeActionMenus();
                            }}
                            className="flex items-center w-full px-3 py-2 text-sm text-popover-foreground hover:bg-accent"
                          >
                            <PencilIcon className="w-4 h-4 mr-2" />
                            编辑
                          </button>
                          <button
                            onClick={async (e) => {
                              e.stopPropagation();
                              closeActionMenus();
                              await onTest(connection);
                            }}
                            className="flex items-center w-full px-3 py-2 text-sm text-popover-foreground hover:bg-accent"
                          >
                            <svg className="w-4 h-4 mr-2" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 12h16M12 4v16" />
                            </svg>
                            测试连接
                          </button>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              onDelete(connection.id);
                              closeActionMenus();
                            }}
                            className="flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                          >
                            <TrashIcon className="w-4 h-4 mr-2" />
                            删除
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

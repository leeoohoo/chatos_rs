import React from 'react';
import { cn } from '../../../lib/utils';
import type { Terminal } from '../../../types';
import { DotsVerticalIcon, PlusIcon, TrashIcon } from '../../ui/icons';

interface TerminalSectionProps {
  expanded: boolean;
  terminals: Terminal[];
  currentTerminalId?: string | null;
  isRefreshing: boolean;
  onToggle: () => void;
  onRefresh: () => void;
  onCreate: () => void;
  onSelect: (terminalId: string) => void;
  onDelete: (terminalId: string) => void;
  onToggleActionMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  closeActionMenus: () => void;
  formatTimeAgo: (date: string | Date | undefined | null) => string;
}

export const TerminalSection: React.FC<TerminalSectionProps> = ({
  expanded,
  terminals,
  currentTerminalId,
  isRefreshing,
  onToggle,
  onRefresh,
  onCreate,
  onSelect,
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
          <span>TERMINALS</span>
        </button>
        <div className="flex items-center gap-1">
          <button
            onClick={onRefresh}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title="刷新终端列表"
          >
            <svg className={cn('w-4 h-4', isRefreshing && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
            </svg>
          </button>
          <button
            type="button"
            onClick={onCreate}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title="新增终端"
          >
            <PlusIcon className="w-4 h-4" />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="flex-1 min-h-0 overflow-y-auto">
          {terminals.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">
              还没有终端，点击右侧 + 新建。
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {terminals.map((terminal) => (
                <div
                  key={terminal.id}
                  className={cn(
                    'group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors',
                    currentTerminalId === terminal.id
                      ? 'bg-accent border border-border'
                      : 'hover:bg-accent/50',
                  )}
                  onClick={() => onSelect(terminal.id)}
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 min-w-0">
                      <h3 className="text-sm font-medium text-foreground truncate min-w-0 flex-1">
                        {terminal.name}
                      </h3>
                      <span className={cn(
                        'inline-flex items-center shrink-0 whitespace-nowrap leading-none text-[10px] px-1.5 py-0.5 rounded border',
                        terminal.status === 'running'
                          ? 'border-emerald-500/40 text-emerald-600'
                          : 'border-muted-foreground/40 text-muted-foreground',
                      )}>
                        {terminal.status === 'running' ? '运行中' : '已退出'}
                      </span>
                      {terminal.status === 'running' && (
                        <span className={cn(
                          'inline-flex items-center shrink-0 whitespace-nowrap leading-none text-[10px] px-1.5 py-0.5 rounded border',
                          terminal.busy
                            ? 'border-amber-500/40 text-amber-600'
                            : 'border-emerald-500/30 text-emerald-600/80',
                        )}>
                          {terminal.busy ? '忙碌' : '空闲'}
                        </span>
                      )}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground truncate" title={terminal.cwd}>
                      {terminal.cwd}
                    </div>
                    {terminal.lastActiveAt && (
                      <div className="mt-1 text-[10px] text-muted-foreground/70">
                        最近活动：{formatTimeAgo(terminal.lastActiveAt)}
                      </div>
                    )}
                  </div>
                  <div className="relative" data-action-menu-root="true">
                    <button
                      className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                      onClick={onToggleActionMenu}
                    >
                      <DotsVerticalIcon className="w-4 h-4" />
                    </button>
                    <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-32 bg-popover border border-border rounded-md shadow-lg">
                      <div className="py-1">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            onDelete(terminal.id);
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
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

import React from 'react';

import { cn } from '../../lib/utils';

export type WorkspaceTab = 'files' | 'team';

interface WorkspaceTabsProps {
  activeTab: WorkspaceTab;
  onChange: (tab: WorkspaceTab) => void;
  rightActions?: React.ReactNode;
}

export const WorkspaceTabs: React.FC<WorkspaceTabsProps> = ({
  activeTab,
  onChange,
  rightActions,
}) => {
  return (
    <div className="border-b border-border bg-card px-3 py-2">
      <div className="flex items-center justify-between gap-3">
        <div className="inline-flex items-center gap-1 rounded-md border border-border bg-background p-1">
          <button
            type="button"
            onClick={() => onChange('files')}
            className={cn(
              'px-3 py-1.5 text-sm rounded transition-colors',
              activeTab === 'files'
                ? 'bg-accent text-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-accent/60'
            )}
          >
            项目目录
          </button>
          <button
            type="button"
            onClick={() => onChange('team')}
            className={cn(
              'px-3 py-1.5 text-sm rounded transition-colors',
              activeTab === 'team'
                ? 'bg-accent text-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-accent/60'
            )}
          >
            团队成员
          </button>
        </div>
        {rightActions && (
          <div className="min-w-0 shrink-0">
            {rightActions}
          </div>
        )}
      </div>
    </div>
  );
};

export default WorkspaceTabs;

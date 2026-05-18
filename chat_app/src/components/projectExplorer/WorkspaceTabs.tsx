import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';

export type WorkspaceTab = 'files' | 'team' | 'settings';

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
  const { t } = useI18n();
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
            {t('projectExplorer.tab.files')}
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
            {t('projectExplorer.tab.team')}
          </button>
          <button
            type="button"
            onClick={() => onChange('settings')}
            className={cn(
              'px-3 py-1.5 text-sm rounded transition-colors',
              activeTab === 'settings'
                ? 'bg-accent text-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-accent/60'
            )}
          >
            {t('projectExplorer.tab.settings')}
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

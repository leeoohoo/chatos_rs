import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';

export type WorkspaceTab = 'files' | 'team' | 'plan' | 'settings';

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
  const tabs: Array<{ id: WorkspaceTab; label: string }> = [
    { id: 'files', label: t('projectExplorer.tab.files') },
    { id: 'team', label: t('projectExplorer.tab.team') },
    { id: 'plan', label: t('projectExplorer.tab.plan') },
    { id: 'settings', label: t('projectExplorer.tab.settings') },
  ];

  return (
    <div className="border-b border-border bg-card px-3 py-2">
      <div className="flex items-center justify-between gap-3">
        <div className="inline-flex items-center gap-1 rounded-md border border-border bg-background p-1">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => onChange(tab.id)}
              className={cn(
                'rounded px-3 py-1.5 text-sm transition-colors',
                activeTab === tab.id
                  ? 'bg-accent text-foreground'
                  : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
              )}
            >
              {tab.label}
            </button>
          ))}
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

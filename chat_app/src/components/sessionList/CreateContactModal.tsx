import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import ManagerFormDialog from '../ui/ManagerFormDialog';

interface CreateContactModalProps {
  isOpen: boolean;
  agents: Array<{
    id: string;
    name: string;
    description?: string;
    enabled?: boolean;
  }>;
  existingAgentIds?: string[];
  selectedAgentId: string | null;
  error: string | null;
  onClose: () => void;
  onSelectedAgentChange: (agentId: string) => void;
  onCreate: () => void;
}

export const CreateContactModal: React.FC<CreateContactModalProps> = ({
  isOpen,
  agents,
  existingAgentIds = [],
  selectedAgentId,
  error,
  onClose,
  onSelectedAgentChange,
  onCreate,
}) => {
  const { t } = useI18n();
  const enabledAgents = agents.filter((agent) => agent.enabled !== false);
  const existingSet = new Set(
    existingAgentIds
      .map((item) => (typeof item === 'string' ? item.trim() : ''))
      .filter((item) => item.length > 0),
  );
  const availableAgents = enabledAgents.filter((agent) => !existingSet.has(agent.id));

  return (
    <ManagerFormDialog
      open={isOpen}
      title={t('contactModal.title')}
      widthClassName="max-w-xl"
      onClose={onClose}
    >
      <div className="space-y-4">
        <div className="space-y-3 rounded-xl border border-border bg-muted/40 p-4">
          {enabledAgents.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              {t('contactModal.noAgents')}
            </div>
          ) : availableAgents.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              {t('contactModal.allAdded')}
            </div>
          ) : (
            <div className="max-h-72 overflow-y-auto rounded-lg border border-border bg-background">
              {availableAgents.map((agent) => {
                const selected = selectedAgentId === agent.id;
                return (
                  <button
                    key={agent.id}
                    type="button"
                    onClick={() => onSelectedAgentChange(agent.id)}
                    className={[
                      'w-full border-b border-border px-3 py-2 text-left last:border-b-0',
                      selected ? 'bg-accent' : 'hover:bg-accent/60',
                    ].join(' ')}
                  >
                    <div className="text-sm font-medium text-foreground">{agent.name}</div>
                    {agent.description ? (
                      <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                        {agent.description}
                      </div>
                    ) : null}
                  </button>
                );
              })}
            </div>
          )}
          {error ? (
            <div className="text-xs text-destructive">{error}</div>
          ) : null}
        </div>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={onCreate}
            disabled={!selectedAgentId || availableAgents.length === 0}
            className="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t('contactModal.submit')}
          </button>
        </div>
      </div>
    </ManagerFormDialog>
  );
};

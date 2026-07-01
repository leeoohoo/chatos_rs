// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../i18n/I18nProvider';
import type { AgentConfig } from '../../types';

interface AgentListProps {
  agents: AgentConfig[];
  onEdit: (agent: AgentConfig) => void;
  onDelete: (agentId: string) => Promise<void>;
}

const AgentList = ({ agents, onEdit, onDelete }: AgentListProps) => {
  const { t } = useI18n();
  if (!agents.length) {
    return (
      <div className="rounded-xl border border-dashed border-border p-6 text-sm text-muted-foreground">
        {t('agentManager.list.empty')}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {agents.map((agent) => (
        <div key={agent.id} className="rounded-xl border border-border bg-background/40 p-4">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h3 className="text-sm font-semibold text-foreground truncate">{agent.name}</h3>
                {agent.ui_status === 'creating' ? (
                  <span className="inline-flex items-center rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-600">
                    {t('agentManager.list.status.creating')}
                  </span>
                ) : (
                  <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-[11px] ${
                    agent.enabled !== false
                      ? 'bg-emerald-500/15 text-emerald-600'
                      : 'bg-muted text-muted-foreground'
                  }`}>
                    {agent.enabled !== false ? t('agentManager.list.status.enabled') : t('agentManager.list.status.disabled')}
                  </span>
                )}
              </div>
              {agent.category ? (
                <div className="mt-1 text-xs text-muted-foreground">{agent.category}</div>
              ) : null}
              {agent.ui_status === 'creating' ? (
                <div className="mt-2 text-sm text-muted-foreground">{t('agentManager.list.creatingHint')}</div>
              ) : null}
              {agent.description ? (
                <div className="mt-2 text-sm text-muted-foreground">{agent.description}</div>
              ) : null}
              <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                <span>{t('agentManager.list.pluginCount', { count: Array.isArray(agent.plugin_sources) ? agent.plugin_sources.length : 0 })}</span>
                <span>{t('agentManager.list.skillCount', { count: Array.isArray(agent.skill_ids) ? agent.skill_ids.length : 0 })}</span>
              </div>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <button
                type="button"
                onClick={() => onEdit(agent)}
                disabled={agent.ui_status === 'creating'}
                className={`px-2.5 py-1.5 text-xs rounded-md transition-colors ${
                  agent.ui_status === 'creating'
                    ? 'bg-muted text-muted-foreground cursor-not-allowed opacity-60'
                    : 'bg-muted hover:bg-accent'
                }`}
              >
                {t('aiModelManager.action.edit')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void onDelete(agent.id);
                }}
                disabled={agent.ui_status === 'creating'}
                className={`px-2.5 py-1.5 text-xs rounded-md transition-colors ${
                  agent.ui_status === 'creating'
                    ? 'bg-muted text-muted-foreground cursor-not-allowed opacity-60'
                    : 'bg-red-500/10 text-red-600 hover:bg-red-500/15'
                }`}
              >
                {t('aiModelManager.action.delete')}
              </button>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
};

export default AgentList;

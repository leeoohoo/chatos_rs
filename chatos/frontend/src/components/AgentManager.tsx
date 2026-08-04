// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useState } from 'react';

import { useI18n } from '../i18n/I18nProvider';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { AgentConfig } from '../types';
import { useDialogService } from './ui/DialogProvider';
import ManagerFormDialog from './ui/ManagerFormDialog';
import AgentAiCreateDialog from './agentManager/AgentAiCreateDialog';
import AgentList from './agentManager/AgentList';
import AgentManagerForm from './agentManager/AgentManagerForm';
import {
  canSubmitAgentForm,
  canSubmitAiCreateAgentForm,
  getDefaultAgentAiCreateFormData,
  getDefaultAgentFormData,
  toAgentFormData,
} from './agentManager/helpers';
import type {
  AgentAiCreateFormData,
  AgentFormData,
  AgentManagerProps,
} from './agentManager/types';

type AgentManagerWindow = Window & {
  __agentManagerInitAt__?: number;
};

const AgentManager: React.FC<AgentManagerProps> = ({ onClose, store: externalStore }) => {
  const { t } = useI18n();
  const internalStoreData = useChatStoreResolved();
  const storeData = externalStore ? externalStore() : internalStoreData;
  const {
    agents,
    aiModelConfigs,
    loadAgents,
    loadAiModelConfigs,
    createAgent,
    updateAgent,
    deleteAgent,
    aiCreateAgent,
  } = storeData;
  const { confirm, alert } = useDialogService();

  const [isFormDialogOpen, setIsFormDialogOpen] = useState(false);
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);
  const [formData, setFormData] = useState<AgentFormData>(getDefaultAgentFormData());
  const [showAiCreate, setShowAiCreate] = useState(false);
  const [aiCreateForm, setAiCreateForm] = useState<AgentAiCreateFormData>(getDefaultAgentAiCreateFormData());
  useEffect(() => {
    const currentWindow = window as AgentManagerWindow;
    const last = currentWindow.__agentManagerInitAt__ || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    currentWindow.__agentManagerInitAt__ = now;
    void loadAgents({ force: true });
    void loadAiModelConfigs({ force: true });
  }, [loadAgents, loadAiModelConfigs]);

  const resetForm = () => {
    setIsFormDialogOpen(false);
    setEditingAgentId(null);
    setFormData(getDefaultAgentFormData());
  };

  const openCreateDialog = () => {
    setEditingAgentId(null);
    setFormData(getDefaultAgentFormData());
    setIsFormDialogOpen(true);
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmitAgentForm(formData)) {
      await alert({
        title: t('agentManager.incompleteTitle'),
        message: t('agentManager.incompleteMessage'),
        type: 'warning',
      });
      return;
    }

    const agent: AgentConfig = {
      id: editingAgentId || '',
      name: formData.name.trim(),
      description: formData.description.trim(),
      category: formData.category.trim(),
      ai_model_config_id: '',
      enabled: formData.enabled,
      role_definition: formData.roleDefinition.trim(),
      plugin_sources: [],
      skill_ids: [],
      default_skill_ids: [],
      createdAt: new Date(),
      updatedAt: new Date(),
      skills: [],
      project_policy: null,
      app_ids: [],
    };

    if (editingAgentId) {
      await updateAgent(agent);
    } else {
      await createAgent(agent);
    }
    resetForm();
  };

  const handleDelete = async (agentId: string) => {
    const confirmed = await confirm({
      title: t('agentManager.deleteTitle'),
      message: t('agentManager.deleteMessage'),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    await deleteAgent(agentId);
  };

  const handleAiCreate = async () => {
    if (!canSubmitAiCreateAgentForm(aiCreateForm)) {
      await alert({
        title: t('agentManager.incompleteTitle'),
        message: t('agentManager.aiIncompleteMessage'),
        type: 'warning',
      });
      return;
    }

    const payload = {
      model_config_id: aiCreateForm.modelConfigId || undefined,
      requirement: aiCreateForm.requirement.trim(),
      name: aiCreateForm.name.trim(),
      category: aiCreateForm.category.trim() || undefined,
      enabled: aiCreateForm.enabled,
    };

    setShowAiCreate(false);
    setAiCreateForm(getDefaultAgentAiCreateFormData());
    const created = await aiCreateAgent(payload);
    if (!created) {
      await alert({
        title: t('agentManager.aiCreateFailedTitle'),
        message: t('agentManager.aiCreateFailedMessage'),
        type: 'danger',
      });
    }
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40" onClick={onClose} />
      <div className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[92vw] max-w-5xl h-[85vh] bg-card z-50 shadow-xl breathing-border flex flex-col rounded-lg">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <div className="flex items-center space-x-3">
            <span className="inline-block w-2.5 h-2.5 rounded-full bg-emerald-500" />
            <h2 className="text-lg font-semibold text-foreground">{t('agentManager.title')}</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title={t('common.close')}
          >
            {t('common.close')}
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          <div className="flex items-center gap-2 pb-2">
            <button
              type="button"
              onClick={openCreateDialog}
              className="px-3 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
            >
              {t('agentManager.action.create')}
            </button>
            <button
              type="button"
              onClick={() => setShowAiCreate(true)}
              className="px-3 py-2 text-sm rounded-lg bg-muted hover:bg-accent transition-colors"
            >
              {t('agentManager.action.aiCreate')}
            </button>
          </div>

          <AgentList
            agents={agents || []}
            onEdit={(agent) => {
              setEditingAgentId(agent.id);
              setFormData(toAgentFormData(agent));
              setIsFormDialogOpen(true);
            }}
            onDelete={handleDelete}
          />
        </div>
      </div>

      <ManagerFormDialog
        open={isFormDialogOpen}
        title={editingAgentId ? t('agentManager.form.titleEdit') : t('agentManager.form.titleCreate')}
        widthClassName="max-w-4xl"
        onClose={resetForm}
      >
        <AgentManagerForm
          editingAgentId={editingAgentId}
          formData={formData}
          showTitle={false}
          onSubmit={handleSubmit}
          onCancel={resetForm}
          onFormDataChange={(patch) => {
            setFormData((current) => ({ ...current, ...patch }));
          }}
        />
      </ManagerFormDialog>

      <AgentAiCreateDialog
        open={showAiCreate}
        formData={aiCreateForm}
        modelOptions={(aiModelConfigs || []).filter((item) => item.enabled)}
        onChange={(patch) => {
          setAiCreateForm((current) => ({ ...current, ...patch }));
        }}
        onCancel={() => {
          setShowAiCreate(false);
        }}
        onSubmit={handleAiCreate}
      />
    </>
  );
};

export default AgentManager;

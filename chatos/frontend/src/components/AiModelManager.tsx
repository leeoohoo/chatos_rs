// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useState, useEffect } from 'react';

import { useI18n } from '../i18n/I18nProvider';
import { useApiClient } from '../lib/api/ApiClientContext';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { AiModelProvider } from '../types';
import { normalizeAiModelProvider } from '../lib/domain/configs';
import { useDialogService } from './ui/DialogProvider';
import ManagerFormDialog from './ui/ManagerFormDialog';
import AiModelList from './aiModelManager/AiModelList';
import AiModelManagerForm from './aiModelManager/AiModelManagerForm';
import {
  canSubmitAiModelForm,
  canSubmitAiModelFormWithOptions,
  getDefaultAiModelFormData,
  toAiModelFormData,
} from './aiModelManager/helpers';
import { BrainIcon, XMarkIcon } from './aiModelManager/icons';
import type { AiModelFormData, AiModelManagerProps } from './aiModelManager/types';

type AiModelManagerWindow = Window & {
  __aiModelManagerInitAt__?: number;
};

const AiModelManager: React.FC<AiModelManagerProps> = ({ onClose, store: externalStore }) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
  const internalStoreData = useChatStoreResolved();
  const storeData = externalStore ? externalStore() : internalStoreData;

  const { loadAiModelConfigs } = storeData;
  const [modelProviders, setModelProviders] = useState<AiModelProvider[]>([]);
  const [isFormDialogOpen, setIsFormDialogOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<AiModelProvider | null>(null);
  const [formData, setFormData] = useState<AiModelFormData>(getDefaultAiModelFormData());
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  const [apiKeyLoading, setApiKeyLoading] = useState(false);
  const [refreshingModels, setRefreshingModels] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const { confirm } = useDialogService();

  const loadModelProviders = useCallback(async () => {
    try {
      const providers = await apiClient.getAiModelProviders();
      setModelProviders(providers.map(normalizeAiModelProvider));
    } catch (error) {
      console.error('Failed to load AI model providers:', error);
      setFormError(error instanceof Error ? error.message : t('aiModelManager.error.loadProviders'));
    }
  }, [apiClient, t]);

  useEffect(() => {
    const currentWindow = window as AiModelManagerWindow;
    const last = currentWindow.__aiModelManagerInitAt__ || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    currentWindow.__aiModelManagerInitAt__ = now;
    void loadAiModelConfigs();
    void loadModelProviders();
  }, [loadAiModelConfigs, loadModelProviders]);

  const resetForm = () => {
    setFormData(getDefaultAiModelFormData());
    setEditingConfig(null);
    setApiKeyVisible(false);
    setApiKeyLoading(false);
    setRefreshingModels(false);
    setFormError(null);
    setIsFormDialogOpen(false);
  };

  const openCreateDialog = () => {
    setEditingConfig(null);
    setFormData(getDefaultAiModelFormData());
    setApiKeyVisible(false);
    setFormError(null);
    setIsFormDialogOpen(true);
  };

  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmitAiModelFormWithOptions(formData, { requireApiKey: true })) {
      return;
    }

    setFormError(null);
    try {
      await apiClient.createAiModelProvider({
        name: formData.name.trim(),
        provider: formData.provider,
        prompt_vendor: formData.prompt_vendor,
        api_key: formData.api_key.trim(),
        base_url: formData.base_url.trim(),
        enabled: formData.enabled,
        supports_images: formData.supports_images,
        supports_reasoning: formData.supports_reasoning,
        supports_responses: formData.supports_responses,
      });
      await Promise.all([
        loadModelProviders(),
        loadAiModelConfigs({ force: true }),
      ]);
      resetForm();
    } catch (error) {
      console.error('Failed to create AI model provider:', error);
      setFormError(error instanceof Error ? error.message : t('aiModelManager.error.saveProvider'));
    }
  };

  const handleEditServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingConfig || !canSubmitAiModelForm(formData)) {
      return;
    }

    setFormError(null);
    try {
      const apiKey = formData.clear_api_key ? '' : formData.api_key.trim();
      await apiClient.updateAiModelProvider(editingConfig.id, {
        name: formData.name.trim(),
        provider: formData.provider,
        prompt_vendor: formData.prompt_vendor,
        base_url: formData.base_url.trim(),
        api_key: apiKey || undefined,
        clear_api_key: formData.clear_api_key,
        enabled: formData.enabled,
        supports_images: formData.supports_images,
        supports_reasoning: formData.supports_reasoning,
        supports_responses: formData.supports_responses,
      });
      await Promise.all([
        loadModelProviders(),
        loadAiModelConfigs({ force: true }),
      ]);
      resetForm();
    } catch (error) {
      console.error('Failed to update AI model provider:', error);
      setFormError(error instanceof Error ? error.message : t('aiModelManager.error.saveProvider'));
    }
  };

  const startEdit = (config: AiModelProvider) => {
    setEditingConfig(config);
    setFormData(toAiModelFormData(config));
    setApiKeyVisible(false);
    setFormError(null);
    setIsFormDialogOpen(true);
  };

  const handleToggleApiKeyVisible = async () => {
    if (apiKeyVisible) {
      setApiKeyVisible(false);
      return;
    }
    if (editingConfig && formData.has_stored_api_key && !formData.api_key.trim()) {
      setApiKeyLoading(true);
      try {
        const detail = await apiClient.getAiModelProvider(editingConfig.id, { includeSecret: true });
        setFormData((current) => ({
          ...current,
          api_key: detail.api_key || '',
          clear_api_key: false,
        }));
      } catch (error) {
        console.error('Failed to reveal AI model API key:', error);
      } finally {
        setApiKeyLoading(false);
      }
    }
    setApiKeyVisible(true);
  };

  const handleRefreshModels = async () => {
    if (!editingConfig || !canSubmitAiModelForm(formData)) {
      return;
    }
    setRefreshingModels(true);
    setFormError(null);
    try {
      const apiKey = formData.clear_api_key ? '' : formData.api_key.trim();
      await apiClient.refreshAiModelProvider(editingConfig.id, {
        id: editingConfig.id,
        name: formData.name.trim(),
        provider: formData.provider,
        prompt_vendor: formData.prompt_vendor,
        base_url: formData.base_url.trim(),
        api_key: apiKey || undefined,
        clear_api_key: formData.clear_api_key,
        enabled: formData.enabled,
        supports_images: formData.supports_images,
        supports_reasoning: formData.supports_reasoning,
        supports_responses: formData.supports_responses,
      });
      await Promise.all([
        loadModelProviders(),
        loadAiModelConfigs({ force: true }),
      ]);
    } catch (error) {
      console.error('Failed to refresh provider models:', error);
      setFormError(error instanceof Error ? error.message : t('aiModelManager.error.refreshProvider'));
    } finally {
      setRefreshingModels(false);
    }
  };

  const handleDeleteServer = async (id: string) => {
    const config = modelProviders.find((c: AiModelProvider) => c.id === id);
    const confirmed = await confirm({
      title: t('aiModelManager.confirmDeleteTitle'),
      message: t('aiModelManager.confirmDeleteMessage', {
        name: config?.name || t('common.unknown'),
      }),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    await apiClient.deleteAiModelProvider(id);
    await Promise.all([
      loadModelProviders(),
      loadAiModelConfigs({ force: true }),
    ]);
  };

  const toggleServerEnabled = async (config: AiModelProvider) => {
    await apiClient.updateAiModelProvider(config.id, {
      enabled: !config.enabled,
    });
    await Promise.all([
      loadModelProviders(),
      loadAiModelConfigs({ force: true }),
    ]);
  };

  const handleFormDataChange = (patch: Partial<AiModelFormData>) => {
    setFormData((current) => ({
      ...current,
      ...patch,
    }));
  };

  return (
    <div className="modal-container">
      <div className="modal-content w-full max-w-2xl max-h-[80vh] overflow-hidden">
        {/* 头部 */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center space-x-3">
            <BrainIcon />
            <h2 className="text-xl font-semibold text-foreground">
              {t('aiModelManager.title')}
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <XMarkIcon />
          </button>
        </div>

        <div className="p-6 overflow-y-auto overflow-x-hidden max-h-[calc(80vh-120px)]">
          <button
            type="button"
            onClick={openCreateDialog}
            className="mb-6 flex w-full items-center justify-center rounded-lg border-2 border-dashed border-border p-4 text-muted-foreground transition-colors hover:border-blue-500 hover:text-blue-600"
          >
            {t('aiModelManager.form.createButton')}
          </button>

          <div className="space-y-3">
            <AiModelList
              aiModelConfigs={modelProviders}
              onToggleEnabled={toggleServerEnabled}
              onEdit={startEdit}
              onDelete={handleDeleteServer}
            />
          </div>
        </div>
      </div>

      <ManagerFormDialog
        open={isFormDialogOpen}
        title={editingConfig ? t('aiModelManager.form.title.edit') : t('aiModelManager.form.title.create')}
        widthClassName="max-w-2xl"
        onClose={resetForm}
      >
        {formError ? (
          <div className="mb-3 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
            {formError}
          </div>
        ) : null}
        <AiModelManagerForm
          editingConfig={editingConfig}
          formData={formData}
          showTitle={false}
          onSubmit={editingConfig ? handleEditServer : handleAddServer}
          onCancel={resetForm}
          onFormDataChange={handleFormDataChange}
          apiKeyVisible={apiKeyVisible}
          apiKeyLoading={apiKeyLoading}
          refreshingModels={refreshingModels}
          onToggleApiKeyVisible={() => void handleToggleApiKeyVisible()}
          onRefreshModels={() => void handleRefreshModels()}
        />
      </ManagerFormDialog>

    </div>
  );
};

export default AiModelManager;

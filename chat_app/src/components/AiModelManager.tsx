import React, { useState, useEffect } from 'react';

import { useI18n } from '../i18n/I18nProvider';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { AiModelConfig } from '../types';
import { useDialogService } from './ui/DialogProvider';
import ManagerFormDialog from './ui/ManagerFormDialog';
import AiModelList from './aiModelManager/AiModelList';
import AiModelManagerForm from './aiModelManager/AiModelManagerForm';
import {
  buildAiModelConfig,
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
  const internalStoreData = useChatStoreResolved();
  const storeData = externalStore ? externalStore() : internalStoreData;

  const { aiModelConfigs, loadAiModelConfigs, updateAiModelConfig, deleteAiModelConfig } = storeData;
  const [isFormDialogOpen, setIsFormDialogOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<AiModelConfig | null>(null);
  const [formData, setFormData] = useState<AiModelFormData>(getDefaultAiModelFormData());
  const { confirm } = useDialogService();

  useEffect(() => {
    const currentWindow = window as AiModelManagerWindow;
    const last = currentWindow.__aiModelManagerInitAt__ || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    currentWindow.__aiModelManagerInitAt__ = now;
    void loadAiModelConfigs();
  }, [loadAiModelConfigs]);

  const resetForm = () => {
    setFormData(getDefaultAiModelFormData());
    setEditingConfig(null);
    setIsFormDialogOpen(false);
  };

  const openCreateDialog = () => {
    setEditingConfig(null);
    setFormData(getDefaultAiModelFormData());
    setIsFormDialogOpen(true);
  };

  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmitAiModelFormWithOptions(formData, { requireApiKey: true })) {
      return;
    }

    await updateAiModelConfig(buildAiModelConfig(formData));
    resetForm();
  };

  const handleEditServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingConfig || !canSubmitAiModelForm(formData)) {
      return;
    }

    await updateAiModelConfig(buildAiModelConfig(formData, editingConfig), {
      clearApiKey: formData.clear_api_key,
    });
    resetForm();
  };

  const startEdit = (config: AiModelConfig) => {
    setEditingConfig(config);
    setFormData(toAiModelFormData(config));
    setIsFormDialogOpen(true);
  };

  const handleDeleteServer = async (id: string) => {
    const config = aiModelConfigs.find((c: AiModelConfig) => c.id === id);
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
    await deleteAiModelConfig(id);
  };

  const toggleServerEnabled = async (config: AiModelConfig) => {
    await updateAiModelConfig({
      ...config,
      enabled: !config.enabled,
      updatedAt: new Date(),
    });
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
              aiModelConfigs={aiModelConfigs}
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
        <AiModelManagerForm
          editingConfig={editingConfig}
          formData={formData}
          showTitle={false}
          onSubmit={editingConfig ? handleEditServer : handleAddServer}
          onCancel={resetForm}
          onFormDataChange={handleFormDataChange}
        />
      </ManagerFormDialog>

    </div>
  );
};

export default AiModelManager;

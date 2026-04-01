import React, { useState, useEffect } from 'react';

import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { AiModelConfig } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import AiModelList from './aiModelManager/AiModelList';
import AiModelManagerForm from './aiModelManager/AiModelManagerForm';
import {
  buildAiModelConfig,
  canSubmitAiModelForm,
  getDefaultAiModelFormData,
  toAiModelFormData,
} from './aiModelManager/helpers';
import { BrainIcon, XMarkIcon } from './aiModelManager/icons';
import type { AiModelFormData, AiModelManagerProps } from './aiModelManager/types';

type AiModelManagerWindow = Window & {
  __aiModelManagerInitAt__?: number;
};

const AiModelManager: React.FC<AiModelManagerProps> = ({ onClose, store: externalStore }) => {
  let storeData;
  if (externalStore) {
    storeData = externalStore();
  } else {
    try {
      storeData = useChatStoreFromContext();
    } catch {
      storeData = useChatStore();
    }
  }

  const { aiModelConfigs, loadAiModelConfigs, updateAiModelConfig, deleteAiModelConfig } = storeData;
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingConfig, setEditingConfig] = useState<AiModelConfig | null>(null);
  const [formData, setFormData] = useState<AiModelFormData>(getDefaultAiModelFormData());
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

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
    setShowAddForm(false);
  };

  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmitAiModelForm(formData)) {
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

    await updateAiModelConfig(buildAiModelConfig(formData, editingConfig));
    resetForm();
  };

  const startEdit = (config: AiModelConfig) => {
    setEditingConfig(config);
    setFormData(toAiModelFormData(config));
    setShowAddForm(true);
  };

  const handleDeleteServer = async (id: string) => {
    const config = aiModelConfigs.find((c: AiModelConfig) => c.id === id);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除AI模型配置 "${config?.name || 'Unknown'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        await deleteAiModelConfig(id);
      }
    });
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
              AI 模型管理
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
          <AiModelManagerForm
            showAddForm={showAddForm}
            editingConfig={editingConfig}
            formData={formData}
            onCreate={() => setShowAddForm(true)}
            onSubmit={editingConfig ? handleEditServer : handleAddServer}
            onCancel={resetForm}
            onFormDataChange={handleFormDataChange}
          />

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

      <ConfirmDialog
        isOpen={dialogState.isOpen}
        title={dialogState.title}
        message={dialogState.message}
        confirmText={dialogState.confirmText}
        cancelText={dialogState.cancelText}
        type={dialogState.type}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </div>
  );
};

export default AiModelManager;

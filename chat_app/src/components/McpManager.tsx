import React, { useState } from 'react';

import { useI18n } from '../i18n/I18nProvider';
import { useApiClient } from '../lib/api/ApiClientContext';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { McpConfig } from '../types';
import { useDialogService } from './ui/DialogProvider';
import ManagerFormDialog from './ui/ManagerFormDialog';
import {
  getDefaultMcpFormData,
  getMcpConfigArgsInput,
  isReadonlyMcpConfig,
  normalizeDynamicConfig,
  parseArgsInput,
} from './mcpManager/helpers';
import {
  ServerIcon,
  XMarkIcon,
} from './mcpManager/icons';
import McpManagerForm from './mcpManager/McpManagerForm';
import McpServerList from './mcpManager/McpServerList';
import type { DynamicConfigRecord, McpFormData, McpManagerProps } from './mcpManager/types';

type McpManagerWindow = Window & {
  __mcpManagerInitAt__?: number;
};

const McpManager: React.FC<McpManagerProps> = ({ onClose, store: externalStore }) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
  const internalStoreData = useChatStoreResolved();
  const storeData = externalStore ? externalStore() : internalStoreData;

  const { mcpConfigs, updateMcpConfig, deleteMcpConfig, loadMcpConfigs } = storeData;
  const { confirm } = useDialogService();

  const [isFormDialogOpen, setIsFormDialogOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<McpConfig | null>(null);
  const [formData, setFormData] = useState<McpFormData>(getDefaultMcpFormData());

  const [configLoading, setConfigLoading] = useState<boolean>(false);
  const [configError, setConfigError] = useState<string | null>(null);
  const [dynamicConfig, setDynamicConfig] = useState<DynamicConfigRecord>({});

  React.useEffect(() => {
    const currentWindow = window as McpManagerWindow;
    const last = currentWindow.__mcpManagerInitAt__ || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    currentWindow.__mcpManagerInitAt__ = now;
    void loadMcpConfigs();
  }, [loadMcpConfigs]);

  const resetForm = () => {
    setFormData(getDefaultMcpFormData());
    setEditingConfig(null);
    setIsFormDialogOpen(false);
    setDynamicConfig({});
    setConfigError(null);
    setConfigLoading(false);
  };

  const openCreateDialog = () => {
    setEditingConfig(null);
    setFormData(getDefaultMcpFormData());
    setDynamicConfig({});
    setConfigError(null);
    setConfigLoading(false);
    setIsFormDialogOpen(true);
  };

  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim() || !formData.command.trim()) {
      return;
    }

    const newConfig: Partial<McpConfig> = {
      id: '',
      name: formData.name.trim(),
      command: formData.command.trim(),
      type: formData.type,
      enabled: true,
      cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
      args: parseArgsInput(formData.argsInput),
      config: Object.keys(dynamicConfig).length > 0 ? dynamicConfig : undefined,
      createdAt: new Date(),
      updatedAt: new Date(),
    };

    await updateMcpConfig(newConfig as McpConfig);
    resetForm();
  };

  const handleEditServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingConfig || !formData.name.trim() || !formData.command.trim()) {
      return;
    }

    const updatedConfig: McpConfig = {
      ...editingConfig,
      name: formData.name.trim(),
      command: formData.command.trim(),
      type: formData.type,
      enabled: true,
      cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
      args: parseArgsInput(formData.argsInput),
      config: Object.keys(dynamicConfig).length > 0 ? dynamicConfig : editingConfig.config,
      updatedAt: new Date(),
    };
    await updateMcpConfig(updatedConfig);
    resetForm();
  };

  const startEdit = (config: McpConfig) => {
    if (isReadonlyMcpConfig(config)) {
      return;
    }
    setEditingConfig(config);
    setFormData({
      name: config.name,
      command: config.command,
      type: config.type || 'stdio',
      cwd: config.cwd || '',
      argsInput: getMcpConfigArgsInput(config),
    });
    setIsFormDialogOpen(true);
    setDynamicConfig(normalizeDynamicConfig(config.config));
    setConfigError(null);
    setConfigLoading(false);
  };

  const handleDeleteServer = async (id: string) => {
    const config = mcpConfigs.find((item: McpConfig) => item.id === id);
    if (isReadonlyMcpConfig(config)) {
      return;
    }
    const confirmed = await confirm({
      title: t('mcpManager.confirmDeleteTitle'),
      message: t('mcpManager.confirmDeleteMessage', {
        name: config?.name || t('common.unknown'),
      }),
      confirmText: t('mcpManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    await deleteMcpConfig(id);
  };

  const handleFetchDynamicConfig = async () => {
    if (!formData.command.trim()) {
      return;
    }

    setConfigLoading(true);
    setConfigError(null);

    try {
      const response = await apiClient.getMcpConfigResourceByCommand({
        type: formData.type,
        command: formData.command.trim(),
        args: parseArgsInput(formData.argsInput),
        env: undefined,
        cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
        alias: null,
      });

      if (response.success && response.config) {
        setDynamicConfig(normalizeDynamicConfig(response.config));
      } else {
        setConfigError(t('mcpManager.configFetchFailed'));
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : t('mcpManager.configFetchError');
      setConfigError(message);
    } finally {
      setConfigLoading(false);
    }
  };

  const handleDynamicConfigChange = (key: string, value: DynamicConfigRecord[string]) => {
    setDynamicConfig((current) => ({
      ...current,
      [key]: value,
    }));
  };

  const handleFormDataChange = (patch: Partial<McpFormData>) => {
    setFormData((current) => ({
      ...current,
      ...patch,
    }));
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40" onClick={onClose} />

      <div className="fixed right-0 top-0 h-full w-[520px] sm:w-[560px] bg-card z-50 shadow-xl breathing-border flex flex-col">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <div className="flex items-center space-x-3">
            <ServerIcon />
            <h2 className="text-lg font-semibold text-foreground">{t('mcpManager.title')}</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title={t('mcpManager.close')}
          >
            <XMarkIcon />
          </button>
        </div>

        <div className="p-4 flex-1 overflow-y-auto overflow-x-hidden">
          <button
            type="button"
            onClick={openCreateDialog}
            className="mb-6 flex w-full items-center justify-center rounded-lg border-2 border-dashed border-border p-4 text-muted-foreground transition-colors hover:border-blue-500 hover:text-blue-600"
          >
            + {t('mcpManager.form.createButton')}
          </button>

          <div className="space-y-3">
            <McpServerList
              mcpConfigs={mcpConfigs}
              onEdit={startEdit}
              onDelete={(id) => void handleDeleteServer(id)}
            />
          </div>
        </div>
      </div>

      <ManagerFormDialog
        open={isFormDialogOpen}
        title={editingConfig ? t('mcpManager.form.title.edit') : t('mcpManager.form.title.create')}
        widthClassName="max-w-2xl"
        onClose={resetForm}
      >
        <McpManagerForm
          editingConfig={editingConfig}
          formData={formData}
          dynamicConfig={dynamicConfig}
          configLoading={configLoading}
          configError={configError}
          showTitle={false}
          onSubmit={editingConfig ? handleEditServer : handleAddServer}
          onCancel={resetForm}
          onFormDataChange={handleFormDataChange}
          onFetchDynamicConfig={handleFetchDynamicConfig}
          onDynamicConfigChange={handleDynamicConfigChange}
        />
      </ManagerFormDialog>
    </>
  );
};

export default McpManager;

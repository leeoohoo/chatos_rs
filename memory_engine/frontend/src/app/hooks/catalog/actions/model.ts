import type { EngineModelProfile } from '../../../../types';
import { api } from '../../../../api';
import { buildModelPayload } from '../../../utils';

import type { CatalogActionsContext, CatalogModelActions } from './types';

export function buildCatalogModelActions(
  context: CatalogActionsContext,
  closeModelModal: () => void,
): CatalogModelActions {
  const { message, controls, loaders } = context;

  const reloadModelsAfterMutation = async (actionLabel: string) => {
    try {
      await loaders.loadModels();
    } catch (error) {
      message.error(`${actionLabel}成功，但刷新模型列表失败：${String(error)}`);
    }
  };

  const notifyAfterModelMutation = async (actionLabel: string) => {
    try {
      await context.callbacks?.afterModelMutation?.();
    } catch (error) {
      message.error(`${actionLabel}成功，但刷新概览统计失败：${String(error)}`);
    }
  };

  const handleSubmitModel = async () => {
    try {
      const values = await controls.modelForm.validateFields();
      const payload = buildModelPayload(values);
      controls.setModelSubmitting(true);
      if (controls.editingModel) {
        await api.updateModelProfile(controls.editingModel.id, payload);
        message.success(`已更新模型配置：${payload.name}`);
      } else {
        await api.createModelProfile(payload);
        message.success(`已创建模型配置：${payload.name}`);
      }
      closeModelModal();
      await reloadModelsAfterMutation('保存模型配置');
      await notifyAfterModelMutation('保存模型配置');
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (!text.includes('out of date') && !text.includes('validate')) {
        message.error(`保存模型配置失败：${text}`);
      }
    } finally {
      controls.setModelSubmitting(false);
    }
  };

  const handleDeleteModel = async (model: EngineModelProfile) => {
    try {
      await api.deleteModelProfile(model.id);
      message.success(`已删除模型配置：${model.name}`);
      await reloadModelsAfterMutation('删除模型配置');
      await notifyAfterModelMutation('删除模型配置');
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      message.error(`删除模型配置失败：${text}`);
    }
  };

  return {
    handleSubmitModel,
    handleDeleteModel,
  };
}

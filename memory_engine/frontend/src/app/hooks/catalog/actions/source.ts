import type { EngineSource } from '../../../../types';
import { api } from '../../../../api';
import { buildSourcePayload } from '../../../utils';

import type { CatalogActionsContext, CatalogSourceActions } from './types';

export function buildCatalogSourceActions(
  context: CatalogActionsContext,
  closeSourceModal: () => void,
): CatalogSourceActions {
  const { message, controls, loaders } = context;

  const reloadSourcesAfterMutation = async (actionLabel: string) => {
    try {
      await loaders.loadSources();
    } catch (error) {
      message.error(`${actionLabel}成功，但刷新接入系统列表失败：${String(error)}`);
    }
  };

  const notifyAfterSourceMutation = async (actionLabel: string) => {
    try {
      await context.callbacks?.afterSourceMutation?.();
    } catch (error) {
      message.error(`${actionLabel}成功，但刷新概览统计失败：${String(error)}`);
    }
  };

  const handleSubmitSource = async () => {
    try {
      const values = await controls.sourceForm.validateFields();
      const { sourceId, payload } = buildSourcePayload(values);
      if (!sourceId) {
        message.error('系统标识不能为空');
        return;
      }
      controls.setSourceSubmitting(true);
      await api.upsertSource(sourceId, payload);
      message.success(
        controls.editingSource
          ? `已更新接入系统：${payload.name}`
          : `已创建接入系统：${payload.name}`,
      );
      closeSourceModal();
      await reloadSourcesAfterMutation('保存接入系统');
      await notifyAfterSourceMutation('保存接入系统');
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (!text.includes('out of date') && !text.includes('validate')) {
        message.error(`保存接入系统失败：${text}`);
      }
    } finally {
      controls.setSourceSubmitting(false);
    }
  };

  const handleRotateSourceSecret = async (source: EngineSource) => {
    try {
      const result = await api.rotateSourceSecret(
        source.source_id,
        source.tenant_id ?? undefined,
      );
      controls.setRotatedSecret(result);
      message.success(`已轮换 SDK Secret：${source.name}`);
      await reloadSourcesAfterMutation('轮换 SDK Secret');
      await notifyAfterSourceMutation('轮换 SDK Secret');
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      message.error(`轮换密钥失败：${text}`);
    }
  };

  return {
    handleSubmitSource,
    handleRotateSourceSecret,
  };
}

import { App } from 'antd';
import { lazy, Suspense, useEffect, useState } from 'react';

import { userServiceApi } from '../../api/userService';
import type { EngineModelProfile } from '../../types';
import { useCatalogResources } from '../hooks/useCatalogResources';
import type { OwnerLabelMap } from './ModelsSection';
import { ModelsSection } from './ModelsSection';

const ModelModal = lazy(() =>
  import('../modals/ModelModal').then((module) => ({ default: module.ModelModal })),
);

type ModelsSectionContainerProps = {
  refreshNonce?: number;
  onCatalogMutated?: () => void | Promise<void>;
};

export function ModelsSectionContainer(props: ModelsSectionContainerProps) {
  const { refreshNonce = 0, onCatalogMutated } = props;
  const { message } = App.useApp();
  const catalog = useCatalogResources(message, {
    afterModelMutation: onCatalogMutated,
  });
  const [ownerLabelsById, setOwnerLabelsById] = useState<OwnerLabelMap>({});

  const loadOwnerLabels = async (models: EngineModelProfile[]) => {
    const ownerIds = Array.from(
      new Set(
        models
          .map((model) => model.owner_user_id?.trim())
          .filter((ownerUserId): ownerUserId is string => Boolean(ownerUserId)),
      ),
    );
    if (ownerIds.length === 0) {
      setOwnerLabelsById({});
      return;
    }
    try {
      const users = await userServiceApi.listUsers();
      const ownerIdSet = new Set(ownerIds);
      setOwnerLabelsById(
        users.reduce<OwnerLabelMap>((acc, user) => {
          if (ownerIdSet.has(user.id)) {
            acc[user.id] = {
              username: user.username,
              display_name: user.display_name,
            };
          }
          return acc;
        }, {}),
      );
    } catch (error) {
      console.warn('Failed to load model owner labels from user_service', error);
      setOwnerLabelsById({});
    }
  };

  const loadModels = async () => {
    try {
      const models = await catalog.loadModels();
      await loadOwnerLabels(models);
    } catch (error) {
      message.error(`加载模型配置失败：${String(error)}`);
    }
  };

  useEffect(() => {
    void loadModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (refreshNonce > 0) {
      void loadModels();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshNonce]);

  return (
    <>
      <ModelsSection
        models={catalog.modelProfiles}
        ownerLabelsById={ownerLabelsById}
        loading={catalog.modelsLoading}
        onReload={() => void loadModels()}
        onCreate={catalog.openCreateModelModal}
        onEdit={catalog.openEditModelModal}
        onDelete={(model) => void catalog.handleDeleteModel(model)}
      />

      {catalog.modelModalOpen ? (
        <Suspense fallback={null}>
          <ModelModal
            open={catalog.modelModalOpen}
            editingModel={catalog.editingModel}
            form={catalog.modelForm}
            submitting={catalog.modelSubmitting}
            onCancel={catalog.closeModelModal}
            onSubmit={() => void catalog.handleSubmitModel()}
          />
        </Suspense>
      ) : null}
    </>
  );
}

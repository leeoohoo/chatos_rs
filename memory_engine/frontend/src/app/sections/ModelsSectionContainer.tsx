import { App } from 'antd';
import { lazy, Suspense, useEffect } from 'react';

import { useCatalogResources } from '../hooks/useCatalogResources';
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

  const loadModels = async () => {
    try {
      await catalog.loadModels();
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

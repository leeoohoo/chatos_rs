// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { App } from 'antd';
import { lazy, Suspense, useEffect } from 'react';

import { useCatalogResources } from '../hooks/useCatalogResources';
import { SourcesSection } from './SourcesSection';

const SourceModal = lazy(() =>
  import('../modals/SourceModal').then((module) => ({ default: module.SourceModal })),
);
const RotatedSecretModal = lazy(() =>
  import('../modals/RotatedSecretModal').then((module) => ({
    default: module.RotatedSecretModal,
  })),
);

type SourcesSectionContainerProps = {
  refreshNonce?: number;
  onCatalogMutated?: () => void | Promise<void>;
};

export function SourcesSectionContainer(props: SourcesSectionContainerProps) {
  const { refreshNonce = 0, onCatalogMutated } = props;
  const { message } = App.useApp();
  const catalog = useCatalogResources(message, {
    afterSourceMutation: onCatalogMutated,
  });

  const loadSources = async () => {
    try {
      await catalog.loadSources();
    } catch (error) {
      message.error(`加载接入系统失败：${String(error)}`);
    }
  };

  useEffect(() => {
    void loadSources();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (refreshNonce > 0) {
      void loadSources();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshNonce]);

  return (
    <>
      <SourcesSection
        sources={catalog.sources}
        loading={catalog.sourcesLoading}
        onReload={() => void loadSources()}
        onCreate={catalog.openCreateSourceModal}
        onEdit={catalog.openEditSourceModal}
        onRotateSecret={(source) => void catalog.handleRotateSourceSecret(source)}
      />

      {catalog.sourceModalOpen ? (
        <Suspense fallback={null}>
          <SourceModal
            open={catalog.sourceModalOpen}
            editingSource={catalog.editingSource}
            form={catalog.sourceForm}
            submitting={catalog.sourceSubmitting}
            onCancel={catalog.closeSourceModal}
            onSubmit={() => void catalog.handleSubmitSource()}
          />
        </Suspense>
      ) : null}

      {catalog.rotatedSecret ? (
        <Suspense fallback={null}>
          <RotatedSecretModal
            rotatedSecret={catalog.rotatedSecret}
            onClose={() => catalog.setRotatedSecret(null)}
          />
        </Suspense>
      ) : null}
    </>
  );
}

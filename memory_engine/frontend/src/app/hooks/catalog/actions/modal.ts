// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineModelProfile, EngineSource } from '../../../../types';
import { modelFormInitialValues, sourceFormInitialValues } from '../../../utils';

import type { CatalogActionControls, CatalogModalActions } from './types';

export function buildCatalogModalActions(
  controls: CatalogActionControls,
): CatalogModalActions {
  const openCreateModelModal = () => {
    controls.setEditingModel(null);
    controls.modelForm.setFieldsValue(modelFormInitialValues(null));
    controls.setModelModalOpen(true);
  };

  const openCreateSourceModal = () => {
    controls.setEditingSource(null);
    controls.sourceForm.setFieldsValue(sourceFormInitialValues(null));
    controls.setSourceModalOpen(true);
  };

  const openEditSourceModal = (source: EngineSource) => {
    controls.setEditingSource(source);
    controls.sourceForm.setFieldsValue(sourceFormInitialValues(source));
    controls.setSourceModalOpen(true);
  };

  const openEditModelModal = (model: EngineModelProfile) => {
    controls.setEditingModel(model);
    controls.modelForm.setFieldsValue(modelFormInitialValues(model));
    controls.setModelModalOpen(true);
  };

  const closeModelModal = () => {
    controls.setModelModalOpen(false);
    controls.setEditingModel(null);
    controls.modelForm.resetFields();
  };

  const closeSourceModal = () => {
    controls.setSourceModalOpen(false);
    controls.setEditingSource(null);
    controls.sourceForm.resetFields();
  };

  return {
    openCreateModelModal,
    openCreateSourceModal,
    openEditSourceModal,
    openEditModelModal,
    closeModelModal,
    closeSourceModal,
  };
}

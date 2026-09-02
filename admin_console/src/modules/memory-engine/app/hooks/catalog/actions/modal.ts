// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineSource } from '../../../../types';
import { sourceFormInitialValues } from '../../../utils';

import type { CatalogActionControls, CatalogModalActions } from './types';

export function buildCatalogModalActions(
  controls: CatalogActionControls,
): CatalogModalActions {
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

  const closeSourceModal = () => {
    controls.setSourceModalOpen(false);
    controls.setEditingSource(null);
    controls.sourceForm.resetFields();
  };

  return {
    openCreateSourceModal,
    openEditSourceModal,
    closeSourceModal,
  };
}

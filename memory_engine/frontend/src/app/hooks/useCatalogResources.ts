// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCatalogActions } from './catalog/useCatalogActions';
import { useCatalogLoaders } from './catalog/useCatalogLoaders';
import { useCatalogState } from './catalog/useCatalogState';
import type { CatalogResourceCallbacks, MessageApi } from './catalog/types';

export function useCatalogResources(
  message: MessageApi,
  callbacks?: CatalogResourceCallbacks,
) {
  const state = useCatalogState();
  const loaders = useCatalogLoaders({
    setSources: state.setSources,
    setModelProfiles: state.setModelProfiles,
    setJobPolicies: state.setJobPolicies,
    setSourcesLoading: state.setSourcesLoading,
    setModelsLoading: state.setModelsLoading,
    setPoliciesLoading: state.setPoliciesLoading,
  });
  const actions = useCatalogActions(
    message,
    {
      editingSource: state.editingSource,
      editingModel: state.editingModel,
      sourceForm: state.sourceForm,
      modelForm: state.modelForm,
      setRotatedSecret: state.setRotatedSecret,
      setSourceSubmitting: state.setSourceSubmitting,
      setModelSubmitting: state.setModelSubmitting,
      setSourceModalOpen: state.setSourceModalOpen,
      setModelModalOpen: state.setModelModalOpen,
      setEditingSource: state.setEditingSource,
      setEditingModel: state.setEditingModel,
      setSavingPolicyJobType: state.setSavingPolicyJobType,
      setGeneratingPolicyJobType: state.setGeneratingPolicyJobType,
    },
    loaders,
    callbacks,
  );

  return {
    ...state,
    ...loaders,
    ...actions,
  };
}

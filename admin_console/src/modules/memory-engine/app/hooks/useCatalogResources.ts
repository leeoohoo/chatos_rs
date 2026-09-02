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
    setJobPolicies: state.setJobPolicies,
    setSourcesLoading: state.setSourcesLoading,
    setPoliciesLoading: state.setPoliciesLoading,
  });
  const actions = useCatalogActions(
    message,
    {
      editingSource: state.editingSource,
      sourceForm: state.sourceForm,
      setRotatedSecret: state.setRotatedSecret,
      setSourceSubmitting: state.setSourceSubmitting,
      setSourceModalOpen: state.setSourceModalOpen,
      setEditingSource: state.setEditingSource,
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

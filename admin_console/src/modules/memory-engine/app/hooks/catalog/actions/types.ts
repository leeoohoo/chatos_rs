// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineModelProfile, EngineSource } from '../../../../types';
import type { PolicyFormValues } from '../../../types';
import type {
  CatalogActions,
  CatalogForms,
  CatalogLoaders,
  CatalogResourceCallbacks,
  CatalogState,
  MessageApi,
} from '../types';

export type CatalogActionControls = Pick<
  CatalogState,
  | 'editingSource'
  | 'editingModel'
  | 'setRotatedSecret'
  | 'setSourceSubmitting'
  | 'setModelSubmitting'
  | 'setSourceModalOpen'
  | 'setModelModalOpen'
  | 'setEditingSource'
  | 'setEditingModel'
  | 'setSavingPolicyJobType'
  | 'setGeneratingPolicyJobType'
> &
  CatalogForms;

export type CatalogModalActions = Pick<
  CatalogActions,
  | 'openCreateModelModal'
  | 'openCreateSourceModal'
  | 'openEditSourceModal'
  | 'openEditModelModal'
  | 'closeModelModal'
  | 'closeSourceModal'
>;

export type CatalogSourceActions = Pick<
  CatalogActions,
  'handleSubmitSource' | 'handleRotateSourceSecret'
>;

export type CatalogModelActions = Pick<
  CatalogActions,
  'handleSubmitModel' | 'handleDeleteModel'
>;

export type CatalogPolicyActions = Pick<
  CatalogActions,
  'handleSavePolicy' | 'handleGeneratePolicyPrompt'
>;

export type CatalogActionsContext = {
  message: MessageApi;
  controls: CatalogActionControls;
  loaders: CatalogLoaders;
  callbacks?: CatalogResourceCallbacks;
};

export type OpenEditSourceModal = (source: EngineSource) => void;
export type OpenEditModelModal = (model: EngineModelProfile) => void;
export type HandleSavePolicy = (
  jobType: string,
  values: PolicyFormValues,
) => Promise<void>;

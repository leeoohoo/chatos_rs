// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineSource } from '../../../../types';
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
  | 'setRotatedSecret'
  | 'setSourceSubmitting'
  | 'setSourceModalOpen'
  | 'setEditingSource'
  | 'setSavingPolicyJobType'
  | 'setGeneratingPolicyJobType'
> &
  CatalogForms;

export type CatalogModalActions = Pick<
  CatalogActions,
  | 'openCreateSourceModal'
  | 'openEditSourceModal'
  | 'closeSourceModal'
>;

export type CatalogSourceActions = Pick<
  CatalogActions,
  'handleSubmitSource' | 'handleRotateSourceSecret'
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
export type HandleSavePolicy = (
  jobType: string,
  values: PolicyFormValues,
) => Promise<void>;

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { App } from 'antd';
import type { Dispatch, SetStateAction } from 'react';

import type {
  EngineJobPolicy,
  EngineJobPolicyCatalog,
  EngineSource,
  RotateSourceSecretResponse,
} from '../../../types';
import type {
  PolicyMap,
  PolicyViewKey,
  SourceFormValues,
} from '../../types';

export type MessageApi = ReturnType<typeof App.useApp>['message'];

export type CatalogState = {
  sourcesLoading: boolean;
  policiesLoading: boolean;
  sourceSubmitting: boolean;
  sourceModalOpen: boolean;
  editingSource: EngineSource | null;
  rotatedSecret: RotateSourceSecretResponse | null;
  sources: EngineSource[];
  jobPolicies: EngineJobPolicy[];
  policyConfigurationStatus: EngineJobPolicyCatalog['configuration_status'] | null;
  selectedPolicyViewKey: PolicyViewKey;
  policyMap: PolicyMap;
  setSourcesLoading: Dispatch<SetStateAction<boolean>>;
  setPoliciesLoading: Dispatch<SetStateAction<boolean>>;
  setSourceSubmitting: Dispatch<SetStateAction<boolean>>;
  setSourceModalOpen: Dispatch<SetStateAction<boolean>>;
  setEditingSource: Dispatch<SetStateAction<EngineSource | null>>;
  setSources: Dispatch<SetStateAction<EngineSource[]>>;
  setJobPolicies: Dispatch<SetStateAction<EngineJobPolicy[]>>;
  setPolicyConfigurationStatus: Dispatch<SetStateAction<EngineJobPolicyCatalog['configuration_status'] | null>>;
  setSelectedPolicyViewKey: Dispatch<SetStateAction<PolicyViewKey>>;
  setRotatedSecret: Dispatch<SetStateAction<RotateSourceSecretResponse | null>>;
};

export type CatalogForms = {
  sourceForm: import('antd').FormInstance<SourceFormValues>;
};

export type CatalogLoaders = {
  loadSources: () => Promise<EngineSource[]>;
  loadPolicies: () => Promise<EngineJobPolicy[]>;
};

export type CatalogResourceCallbacks = {
  afterSourceMutation?: () => void | Promise<void>;
};

export type CatalogActions = {
  openCreateSourceModal: () => void;
  openEditSourceModal: (source: EngineSource) => void;
  closeSourceModal: () => void;
  handleSubmitSource: () => Promise<void>;
  handleRotateSourceSecret: (source: EngineSource) => Promise<void>;
};

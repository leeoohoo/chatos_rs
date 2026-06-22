import { App } from 'antd';
import type { Dispatch, SetStateAction } from 'react';

import type {
  EngineJobPolicy,
  EngineModelProfile,
  EngineSource,
  RotateSourceSecretResponse,
} from '../../../types';
import type {
  JobTypeKey,
  ModelFormValues,
  PolicyFormValues,
  PolicyMap,
  PolicyViewKey,
  SourceFormValues,
} from '../../types';

export type MessageApi = ReturnType<typeof App.useApp>['message'];

export type CatalogState = {
  sourcesLoading: boolean;
  modelsLoading: boolean;
  policiesLoading: boolean;
  sourceSubmitting: boolean;
  modelSubmitting: boolean;
  sourceModalOpen: boolean;
  modelModalOpen: boolean;
  editingSource: EngineSource | null;
  editingModel: EngineModelProfile | null;
  rotatedSecret: RotateSourceSecretResponse | null;
  savingPolicyJobType: string | null;
  generatingPolicyJobType: string | null;
  sources: EngineSource[];
  modelProfiles: EngineModelProfile[];
  jobPolicies: EngineJobPolicy[];
  selectedPolicyViewKey: PolicyViewKey;
  policyMap: PolicyMap;
  modelOptions: Array<{ label: string; value: string }>;
  setSourcesLoading: Dispatch<SetStateAction<boolean>>;
  setModelsLoading: Dispatch<SetStateAction<boolean>>;
  setPoliciesLoading: Dispatch<SetStateAction<boolean>>;
  setSourceSubmitting: Dispatch<SetStateAction<boolean>>;
  setModelSubmitting: Dispatch<SetStateAction<boolean>>;
  setSourceModalOpen: Dispatch<SetStateAction<boolean>>;
  setModelModalOpen: Dispatch<SetStateAction<boolean>>;
  setEditingSource: Dispatch<SetStateAction<EngineSource | null>>;
  setEditingModel: Dispatch<SetStateAction<EngineModelProfile | null>>;
  setSavingPolicyJobType: Dispatch<SetStateAction<string | null>>;
  setGeneratingPolicyJobType: Dispatch<SetStateAction<string | null>>;
  setSources: Dispatch<SetStateAction<EngineSource[]>>;
  setModelProfiles: Dispatch<SetStateAction<EngineModelProfile[]>>;
  setJobPolicies: Dispatch<SetStateAction<EngineJobPolicy[]>>;
  setSelectedPolicyViewKey: Dispatch<SetStateAction<PolicyViewKey>>;
  setRotatedSecret: Dispatch<SetStateAction<RotateSourceSecretResponse | null>>;
};

export type CatalogForms = {
  sourceForm: import('antd').FormInstance<SourceFormValues>;
  modelForm: import('antd').FormInstance<ModelFormValues>;
};

export type CatalogLoaders = {
  loadSources: () => Promise<EngineSource[]>;
  loadModels: () => Promise<EngineModelProfile[]>;
  loadPolicies: () => Promise<EngineJobPolicy[]>;
};

export type CatalogResourceCallbacks = {
  afterSourceMutation?: () => void | Promise<void>;
  afterModelMutation?: () => void | Promise<void>;
};

export type CatalogActions = {
  openCreateModelModal: () => void;
  openCreateSourceModal: () => void;
  openEditSourceModal: (source: EngineSource) => void;
  openEditModelModal: (model: EngineModelProfile) => void;
  closeModelModal: () => void;
  closeSourceModal: () => void;
  handleSubmitSource: () => Promise<void>;
  handleRotateSourceSecret: (source: EngineSource) => Promise<void>;
  handleSubmitModel: () => Promise<void>;
  handleDeleteModel: (model: EngineModelProfile) => Promise<void>;
  handleSavePolicy: (jobType: string, values: PolicyFormValues) => Promise<void>;
  handleGeneratePolicyPrompt: (
    jobType: string,
    promptField: 'summary_prompt' | 'rollup_summary_prompt',
    userInput: string,
  ) => Promise<{ prompt_zh: string; prompt_en: string }>;
};

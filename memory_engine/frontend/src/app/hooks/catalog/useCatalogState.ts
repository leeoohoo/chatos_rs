import { Form } from 'antd';
import { useEffect, useMemo, useState } from 'react';

import type {
  EngineJobPolicy,
  EngineModelProfile,
  EngineSource,
  RotateSourceSecretResponse,
} from '../../../types';
import type {
  JobTypeKey,
  ModelFormValues,
  PolicyMap,
  PolicyViewKey,
  SourceFormValues,
} from '../../types';
import type { CatalogForms, CatalogState } from './types';

const POLICY_VIEW_KEYS: PolicyViewKey[] = [
  'summary',
  'rollup',
  'memory_from_summary',
  'memory_rollup',
  'thread_repair',
];

export function useCatalogState(): CatalogState & CatalogForms {
  const [sourcesLoading, setSourcesLoading] = useState(false);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [policiesLoading, setPoliciesLoading] = useState(false);
  const [sourceSubmitting, setSourceSubmitting] = useState(false);
  const [modelSubmitting, setModelSubmitting] = useState(false);
  const [sourceModalOpen, setSourceModalOpen] = useState(false);
  const [modelModalOpen, setModelModalOpen] = useState(false);
  const [editingSource, setEditingSource] = useState<EngineSource | null>(null);
  const [editingModel, setEditingModel] = useState<EngineModelProfile | null>(null);
  const [rotatedSecret, setRotatedSecret] = useState<RotateSourceSecretResponse | null>(null);
  const [savingPolicyJobType, setSavingPolicyJobType] = useState<string | null>(null);
  const [generatingPolicyJobType, setGeneratingPolicyJobType] = useState<string | null>(null);
  const [sources, setSources] = useState<EngineSource[]>([]);
  const [modelProfiles, setModelProfiles] = useState<EngineModelProfile[]>([]);
  const [jobPolicies, setJobPolicies] = useState<EngineJobPolicy[]>([]);
  const [selectedPolicyViewKey, setSelectedPolicyViewKey] =
    useState<PolicyViewKey>('summary');

  const [sourceForm] = Form.useForm<SourceFormValues>();
  const [modelForm] = Form.useForm<ModelFormValues>();

  const policyMap = useMemo(
    () =>
      jobPolicies.reduce<PolicyMap>((acc, policy) => {
        if (
          policy.job_type === 'summary' ||
          policy.job_type === 'rollup' ||
          policy.job_type === 'subject_memory' ||
          policy.job_type === 'thread_repair'
        ) {
          acc[policy.job_type as JobTypeKey] = policy;
        }
        return acc;
      }, {}),
    [jobPolicies],
  );

  useEffect(() => {
    const availableKeys = POLICY_VIEW_KEYS.filter((key) => {
      if (key === 'summary') return Boolean(policyMap.summary);
      if (key === 'rollup') return Boolean(policyMap.rollup);
      if (key === 'thread_repair') return Boolean(policyMap.thread_repair);
      return Boolean(policyMap.subject_memory);
    });
    if (availableKeys.length === 0) {
      return;
    }
    if (!availableKeys.includes(selectedPolicyViewKey)) {
      setSelectedPolicyViewKey(availableKeys[0]);
    }
  }, [policyMap, selectedPolicyViewKey]);

  const modelOptions = modelProfiles.map((profile) => ({
    label: `${profile.name} (${profile.model})`,
    value: profile.id,
  }));

  return {
    sourcesLoading,
    modelsLoading,
    policiesLoading,
    sourceSubmitting,
    modelSubmitting,
    sourceModalOpen,
    modelModalOpen,
    editingSource,
    editingModel,
    rotatedSecret,
    savingPolicyJobType,
    generatingPolicyJobType,
    sources,
    modelProfiles,
    jobPolicies,
    selectedPolicyViewKey,
    sourceForm,
    modelForm,
    policyMap,
    modelOptions,
    setSourcesLoading,
    setModelsLoading,
    setPoliciesLoading,
    setSourceSubmitting,
    setModelSubmitting,
    setSourceModalOpen,
    setModelModalOpen,
    setEditingSource,
    setEditingModel,
    setSavingPolicyJobType,
    setGeneratingPolicyJobType,
    setSources,
    setModelProfiles,
    setJobPolicies,
    setSelectedPolicyViewKey,
    setRotatedSecret,
  };
}

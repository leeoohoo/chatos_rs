// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Form } from 'antd';
import { useEffect, useMemo, useState } from 'react';

import type { EngineJobPolicy, EngineSource, RotateSourceSecretResponse } from '../../../types';
import type {
  JobTypeKey,
  PolicyMap,
  PolicyViewKey,
  SourceFormValues,
} from '../../types';
import type { CatalogForms, CatalogState } from './types';

const POLICY_VIEW_KEYS: PolicyViewKey[] = [
  'summary',
  'rollup',
  'subject_memory',
];

export function useCatalogState(): CatalogState & CatalogForms {
  const [sourcesLoading, setSourcesLoading] = useState(false);
  const [policiesLoading, setPoliciesLoading] = useState(false);
  const [sourceSubmitting, setSourceSubmitting] = useState(false);
  const [sourceModalOpen, setSourceModalOpen] = useState(false);
  const [editingSource, setEditingSource] = useState<EngineSource | null>(null);
  const [rotatedSecret, setRotatedSecret] = useState<RotateSourceSecretResponse | null>(null);
  const [sources, setSources] = useState<EngineSource[]>([]);
  const [jobPolicies, setJobPolicies] = useState<EngineJobPolicy[]>([]);
  const [selectedPolicyViewKey, setSelectedPolicyViewKey] =
    useState<PolicyViewKey>('summary');

  const [sourceForm] = Form.useForm<SourceFormValues>();

  const policyMap = useMemo(
    () =>
      jobPolicies.reduce<PolicyMap>((acc, policy) => {
        if (
          policy.job_type === 'summary' ||
          policy.job_type === 'rollup' ||
          policy.job_type === 'subject_memory'
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
      return Boolean(policyMap.subject_memory);
    });
    if (availableKeys.length === 0) {
      return;
    }
    if (!availableKeys.includes(selectedPolicyViewKey)) {
      setSelectedPolicyViewKey(availableKeys[0]);
    }
  }, [policyMap, selectedPolicyViewKey]);

  return {
    sourcesLoading,
    policiesLoading,
    sourceSubmitting,
    sourceModalOpen,
    editingSource,
    rotatedSecret,
    sources,
    jobPolicies,
    selectedPolicyViewKey,
    sourceForm,
    policyMap,
    setSourcesLoading,
    setPoliciesLoading,
    setSourceSubmitting,
    setSourceModalOpen,
    setEditingSource,
    setSources,
    setJobPolicies,
    setSelectedPolicyViewKey,
    setRotatedSecret,
  };
}

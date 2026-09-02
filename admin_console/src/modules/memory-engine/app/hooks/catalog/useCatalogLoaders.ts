// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useRef } from 'react';

import { api } from '../../../api';
import type { CatalogState } from './types';

type CatalogLoadingControls = Pick<
  CatalogState,
  'setSources' | 'setJobPolicies'
> & {
  setSourcesLoading: (value: boolean) => void;
  setPoliciesLoading: (value: boolean) => void;
};

export function useCatalogLoaders(controls: CatalogLoadingControls) {
  const sourcesRequestIdRef = useRef(0);
  const policiesRequestIdRef = useRef(0);

  const loadSources = async () => {
    const requestId = sourcesRequestIdRef.current + 1;
    sourcesRequestIdRef.current = requestId;
    controls.setSourcesLoading(true);
    try {
      const items = await api.listSources();
      if (sourcesRequestIdRef.current !== requestId) {
        return [];
      }
      controls.setSources(items);
      return items;
    } finally {
      if (sourcesRequestIdRef.current === requestId) {
        controls.setSourcesLoading(false);
      }
    }
  };

  const loadPolicies = async () => {
    const requestId = policiesRequestIdRef.current + 1;
    policiesRequestIdRef.current = requestId;
    controls.setPoliciesLoading(true);
    try {
      const policies = await api.listJobPolicies();
      if (policiesRequestIdRef.current !== requestId) {
        return [];
      }
      controls.setJobPolicies(policies);
      return policies;
    } finally {
      if (policiesRequestIdRef.current === requestId) {
        controls.setPoliciesLoading(false);
      }
    }
  };

  return {
    loadSources,
    loadPolicies,
  };
}

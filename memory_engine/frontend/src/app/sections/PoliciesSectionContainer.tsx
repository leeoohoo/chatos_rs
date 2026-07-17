// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { App } from 'antd';
import { useEffect } from 'react';

import { useCatalogResources } from '../hooks/useCatalogResources';
import { PoliciesSection } from './PoliciesSection';

type PoliciesSectionContainerProps = {
  refreshNonce?: number;
};

export function PoliciesSectionContainer(props: PoliciesSectionContainerProps) {
  const { refreshNonce = 0 } = props;
  const { message } = App.useApp();
  const catalog = useCatalogResources(message);

  const loadPoliciesPage = async () => {
    try {
      await catalog.loadPolicies();
    } catch (error) {
      message.error(`加载任务策略失败：${String(error)}`);
    }
  };

  useEffect(() => {
    void loadPoliciesPage();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (refreshNonce > 0) {
      void loadPoliciesPage();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshNonce]);

  return (
    <PoliciesSection
      policies={catalog.jobPolicies}
      loading={catalog.modelsLoading || catalog.policiesLoading}
      selectedKey={catalog.selectedPolicyViewKey}
      onSelect={catalog.setSelectedPolicyViewKey}
      onReload={() => void loadPoliciesPage()}
      policyMap={catalog.policyMap}
    />
  );
}

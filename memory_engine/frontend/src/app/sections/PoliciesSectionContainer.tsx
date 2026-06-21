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
    const [models, policies] = await Promise.allSettled([
      catalog.loadModels(),
      catalog.loadPolicies(),
    ]);

    if (models.status === 'rejected') {
      message.error(`加载模型配置失败：${String(models.reason)}`);
    }

    if (policies.status === 'rejected') {
      message.error(`加载任务策略失败：${String(policies.reason)}`);
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
      modelOptions={catalog.modelOptions}
      savingPolicyJobType={catalog.savingPolicyJobType}
      generatingPolicyJobType={catalog.generatingPolicyJobType}
      onSave={catalog.handleSavePolicy}
      onGeneratePrompt={catalog.handleGeneratePolicyPrompt}
    />
  );
}

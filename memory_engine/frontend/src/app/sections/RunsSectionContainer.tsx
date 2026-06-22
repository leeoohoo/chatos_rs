import { App } from 'antd';
import { useEffect } from 'react';

import { useRunManagement } from '../hooks/useRunManagement';
import { RunsSection } from './RunsSection';

type RunsSectionContainerProps = {
  refreshNonce?: number;
};

export function RunsSectionContainer(props: RunsSectionContainerProps) {
  const { refreshNonce = 0 } = props;
  const { message } = App.useApp();
  const runManagement = useRunManagement({
    onError: (error) => {
      message.error(error);
    },
  });

  useEffect(() => {
    void runManagement.loadRuns();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (refreshNonce > 0) {
      void runManagement.loadRuns();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshNonce]);

  return (
    <RunsSection
      form={runManagement.runFilterForm}
      initialValues={runManagement.runFilters}
      loading={runManagement.runsLoading}
      onApply={() => void runManagement.handleApplyRunFilters()}
      onReset={() => void runManagement.handleResetRunFilters()}
      threadRuns={runManagement.threadJobRuns}
      schedulerRuns={runManagement.schedulerJobRuns}
    />
  );
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { App } from 'antd';
import { useEffect, useState } from 'react';

import { userServiceApi } from '../../api/userService';
import { useAgentExplorer } from '../hooks/useAgentExplorer';
import { DataSection } from './DataSection';
import type { UserLabelMap } from './data/types';

type DataSectionContainerProps = {
  refreshNonce?: number;
};

export function DataSectionContainer(props: DataSectionContainerProps) {
  const { refreshNonce = 0 } = props;
  const { message } = App.useApp();
  const [tenantLabelsById, setTenantLabelsById] = useState<UserLabelMap>({});
  const agentExplorer = useAgentExplorer({
    onError: (error) => {
      message.error(error);
    },
  });

  useEffect(() => {
    if (refreshNonce > 0) {
      void agentExplorer.loadAgents();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshNonce]);

  useEffect(() => {
    const tenantIds = Array.from(
      new Set(
        agentExplorer.agents
          .map((agent) => agent.tenant_id?.trim())
          .filter((tenantId): tenantId is string => Boolean(tenantId)),
      ),
    );
    if (tenantIds.length === 0) {
      setTenantLabelsById({});
      return;
    }

    let cancelled = false;
    const tenantIdSet = new Set(tenantIds);

    const loadTenantLabels = async () => {
      try {
        const users = await userServiceApi.listUserOptions(tenantIds);
        if (cancelled) {
          return;
        }
        setTenantLabelsById(
          users.reduce<UserLabelMap>((acc, user) => {
            if (tenantIdSet.has(user.id)) {
              acc[user.id] = {
                username: user.username,
                display_name: user.display_name,
              };
            }
            return acc;
          }, {}),
        );
      } catch (error) {
        if (!cancelled) {
          console.warn('Failed to load memory tenant labels from user_service', error);
          setTenantLabelsById({});
        }
      }
    };

    void loadTenantLabels();

    return () => {
      cancelled = true;
    };
  }, [agentExplorer.agents]);

  const threadExplorer = agentExplorer.threadExplorer;

  return (
    <DataSection
      form={agentExplorer.filterForm}
      initialValues={agentExplorer.filters}
      loading={agentExplorer.agentsLoading}
      threadsLoading={threadExplorer.threadsLoading}
      threadDetailLoading={threadExplorer.threadDetailLoading}
      threadRecordsLoading={threadExplorer.threadRecordsLoading}
      onApplyFilters={() => void agentExplorer.applyFilters()}
      onResetFilters={() => void agentExplorer.resetFilters()}
      onReload={() => void agentExplorer.loadAgents()}
      agents={agentExplorer.agents}
      agentsLoading={agentExplorer.agentsLoading}
      selectedAgent={agentExplorer.selectedAgent}
      onSelectAgent={(agent) => void agentExplorer.selectAgent(agent)}
      projects={agentExplorer.projects}
      conversations={agentExplorer.conversations}
      selectedConversation={agentExplorer.selectedConversation}
      onSelectConversation={(conversation) => void agentExplorer.selectConversation(conversation)}
      selectedProject={agentExplorer.selectedProject}
      onSelectProject={(project) => void agentExplorer.selectProject(project)}
      agentMemories={agentExplorer.agentMemories}
      projectMemories={agentExplorer.projectMemories}
      agentDetailLoading={agentExplorer.agentDetailLoading}
      projectMemoriesLoading={agentExplorer.projectMemoriesLoading}
      threads={threadExplorer.threads}
      tenantLabelsById={tenantLabelsById}
      threadPage={threadExplorer.threadPage}
      threadPageSize={threadExplorer.threadPageSize}
      threadHasMore={threadExplorer.threadHasMore}
      threadMaxReachablePage={threadExplorer.threadMaxReachablePage}
      onThreadPageChange={(page, pageSize) =>
        void threadExplorer.handleThreadPageChange(page, pageSize)
      }
      selectedThread={threadExplorer.selectedThread}
      onSelectThread={(thread) =>
        void threadExplorer.loadThreadDetails(thread, { resetPage: true })
      }
      threadRecords={threadExplorer.threadRecords}
      threadRecordPage={threadExplorer.threadRecordPage}
      threadRecordPageSize={threadExplorer.threadRecordPageSize}
      threadRecordTotal={threadExplorer.threadRecordTotal}
      threadRecordMaxReachablePage={threadExplorer.threadRecordMaxReachablePage}
      onThreadRecordPageChange={(page, pageSize) =>
        void threadExplorer.handleThreadRecordPageChange(page, pageSize)
      }
      threadSummaries={threadExplorer.threadSummaries}
      threadSummaryPage={threadExplorer.threadSummaryPage}
      threadSummaryPageSize={threadExplorer.threadSummaryPageSize}
      threadSummaryHasMore={threadExplorer.threadSummaryHasMore}
      threadSummaryMaxReachablePage={threadExplorer.threadSummaryMaxReachablePage}
      onThreadSummaryPageChange={(page, pageSize) =>
        void threadExplorer.handleThreadSummaryPageChange(page, pageSize)
      }
      subjectMemories={threadExplorer.subjectMemories}
      detailTab={threadExplorer.detailTab}
      onDetailTabChange={(detailTab) => threadExplorer.setDetailTab(detailTab)}
    />
  );
}

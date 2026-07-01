// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { App } from 'antd';
import { useEffect, useState } from 'react';

import { userServiceApi } from '../../api/userService';
import { useThreadExplorer } from '../hooks/useThreadExplorer';
import { DataSection } from './DataSection';
import type { UserLabelMap } from './data/types';

type DataSectionContainerProps = {
  refreshNonce?: number;
};

export function DataSectionContainer(props: DataSectionContainerProps) {
  const { refreshNonce = 0 } = props;
  const { message } = App.useApp();
  const [tenantLabelsById, setTenantLabelsById] = useState<UserLabelMap>({});
  const threadExplorer = useThreadExplorer('data', {
    onError: (error) => {
      message.error(error);
    },
  });

  useEffect(() => {
    if (refreshNonce > 0) {
      void threadExplorer.loadThreads();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshNonce]);

  useEffect(() => {
    const tenantIds = Array.from(
      new Set(
        threadExplorer.threads
          .map((thread) => thread.tenant_id?.trim())
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
        const users = await userServiceApi.listUsers();
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
  }, [threadExplorer.threads]);

  return (
    <DataSection
      form={threadExplorer.threadFilterForm}
      initialValues={threadExplorer.threadFilters}
      threadsLoading={threadExplorer.threadsLoading}
      threadDetailLoading={threadExplorer.threadDetailLoading}
      threadRecordsLoading={threadExplorer.threadRecordsLoading}
      onApplyFilters={() => void threadExplorer.handleApplyThreadFilters()}
      onResetFilters={() => void threadExplorer.handleResetThreadFilters()}
      onReload={() => void threadExplorer.loadThreads()}
      threads={threadExplorer.threads}
      tenantLabelsById={tenantLabelsById}
      selectedThread={threadExplorer.selectedThread}
      onSelectThread={(thread) =>
        void threadExplorer.loadThreadDetails(thread, { resetPage: true })
      }
      threadRecords={threadExplorer.threadRecords}
      threadRecordPage={threadExplorer.threadRecordPage}
      threadRecordPageSize={threadExplorer.threadRecordPageSize}
      threadRecordTotal={threadExplorer.threadRecordTotal}
      onThreadRecordPageChange={(page, pageSize) =>
        void threadExplorer.handleThreadRecordPageChange(page, pageSize)
      }
      threadSummaries={threadExplorer.threadSummaries}
      subjectMemories={threadExplorer.subjectMemories}
      detailTab={threadExplorer.detailTab}
      onDetailTabChange={(detailTab) => threadExplorer.setDetailTab(detailTab)}
    />
  );
}

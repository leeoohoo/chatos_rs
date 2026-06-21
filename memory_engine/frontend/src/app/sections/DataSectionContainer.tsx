import { App } from 'antd';
import { useEffect } from 'react';

import { useThreadExplorer } from '../hooks/useThreadExplorer';
import { DataSection } from './DataSection';

type DataSectionContainerProps = {
  refreshNonce?: number;
};

export function DataSectionContainer(props: DataSectionContainerProps) {
  const { refreshNonce = 0 } = props;
  const { message } = App.useApp();
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

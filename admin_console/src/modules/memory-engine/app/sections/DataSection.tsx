// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DataFiltersCard } from './data/DataFiltersCard';
import { ThreadWorkspace } from './data/ThreadWorkspace';
import type { DataSectionProps } from './data/types';

export function DataSection(props: DataSectionProps) {
  return (
    <div className="engine-data-page">
      <DataFiltersCard
        form={props.form}
        initialValues={props.initialValues}
        threadsLoading={props.threadsLoading}
        onApplyFilters={props.onApplyFilters}
        onResetFilters={props.onResetFilters}
        onReload={props.onReload}
      />
      <ThreadWorkspace
        threads={props.threads}
        tenantLabelsById={props.tenantLabelsById}
        threadsLoading={props.threadsLoading}
        threadPage={props.threadPage}
        threadPageSize={props.threadPageSize}
        threadHasMore={props.threadHasMore}
        threadMaxReachablePage={props.threadMaxReachablePage}
        onThreadPageChange={props.onThreadPageChange}
        selectedThread={props.selectedThread}
        onSelectThread={props.onSelectThread}
        threadDetailLoading={props.threadDetailLoading}
        threadRecordsLoading={props.threadRecordsLoading}
        threadRecords={props.threadRecords}
        threadRecordPage={props.threadRecordPage}
        threadRecordPageSize={props.threadRecordPageSize}
        threadRecordTotal={props.threadRecordTotal}
        threadRecordMaxReachablePage={props.threadRecordMaxReachablePage}
        onThreadRecordPageChange={props.onThreadRecordPageChange}
        threadSummaries={props.threadSummaries}
        threadSummaryPage={props.threadSummaryPage}
        threadSummaryPageSize={props.threadSummaryPageSize}
        threadSummaryHasMore={props.threadSummaryHasMore}
        threadSummaryMaxReachablePage={props.threadSummaryMaxReachablePage}
        onThreadSummaryPageChange={props.onThreadSummaryPageChange}
        subjectMemories={props.subjectMemories}
        detailTab={props.detailTab}
        onDetailTabChange={props.onDetailTabChange}
      />
    </div>
  );
}

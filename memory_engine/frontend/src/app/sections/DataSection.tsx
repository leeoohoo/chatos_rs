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
        threadsLoading={props.threadsLoading}
        selectedThread={props.selectedThread}
        onSelectThread={props.onSelectThread}
        threadDetailLoading={props.threadDetailLoading}
        threadRecordsLoading={props.threadRecordsLoading}
        threadRecords={props.threadRecords}
        threadRecordPage={props.threadRecordPage}
        threadRecordPageSize={props.threadRecordPageSize}
        threadRecordTotal={props.threadRecordTotal}
        onThreadRecordPageChange={props.onThreadRecordPageChange}
        threadSummaries={props.threadSummaries}
        subjectMemories={props.subjectMemories}
        detailTab={props.detailTab}
        onDetailTabChange={props.onDetailTabChange}
      />
    </div>
  );
}

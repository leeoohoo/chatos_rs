import { Empty, Table } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { TableRowSelection } from 'antd/es/table/interface';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRecord } from '../../types';

type TaskListTableProps = {
  t: TranslateFn;
  selectedTaskIds: string[];
  loading: boolean;
  columns: ColumnsType<TaskRecord>;
  tasks: TaskRecord[];
  page: number;
  pageSize: number;
  total: number;
  onSelectedTaskIdsChange: (taskIds: string[]) => void;
  onPageChange: (page: number, pageSize: number) => void;
};

export function TaskListTable({
  t,
  selectedTaskIds,
  loading,
  columns,
  tasks,
  page,
  pageSize,
  total,
  onSelectedTaskIdsChange,
  onPageChange,
}: TaskListTableProps) {
  const rowSelection: TableRowSelection<TaskRecord> = {
    selectedRowKeys: selectedTaskIds,
    onChange: (selectedRowKeys) => onSelectedTaskIdsChange(selectedRowKeys.map(String)),
  };

  return (
    <Table<TaskRecord>
      rowKey="id"
      rowSelection={rowSelection}
      loading={loading}
      columns={columns}
      dataSource={tasks}
      pagination={{
        current: page,
        pageSize,
        total,
        showSizeChanger: true,
        onChange: onPageChange,
      }}
      scroll={{ x: 1460 }}
      locale={{
        emptyText: (
          <Empty
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            description={t('tasks.empty')}
          />
        ),
      }}
    />
  );
}

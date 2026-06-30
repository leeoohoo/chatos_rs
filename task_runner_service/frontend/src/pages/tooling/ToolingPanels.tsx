import { useMemo } from 'react';
import {
  Button,
  Empty,
  Input,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ToolingNoteSummary, ToolingTagCount, ToolingTerminalProcessRecord } from '../../types';
import { buildToolingNoteColumns, buildToolingTerminalColumns } from './toolingPageUtils';

type ToolingNotepadPanelProps = {
  t: TranslateFn;
  userId: string;
  folderFilter?: string;
  tagFilter: string[];
  queryText: string;
  folders: string[];
  tags: ToolingTagCount[];
  notes: ToolingNoteSummary[];
  loading: boolean;
  onUserIdChange: (value: string) => void;
  onFolderFilterChange: (value?: string) => void;
  onTagFilterChange: (value: string[]) => void;
  onQueryTextChange: (value: string) => void;
  onClearFilters: () => void;
  onRefresh: () => void;
  onOpenNote: (noteId: string) => void;
};

export function ToolingNotepadPanel({
  t,
  userId,
  folderFilter,
  tagFilter,
  queryText,
  folders,
  tags,
  notes,
  loading,
  onUserIdChange,
  onFolderFilterChange,
  onTagFilterChange,
  onQueryTextChange,
  onClearFilters,
  onRefresh,
  onOpenNote,
}: ToolingNotepadPanelProps) {
  const columns = useMemo(
    () => buildToolingNoteColumns({ t, onOpenNote }),
    [t, onOpenNote],
  );

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space wrap>
        <Input
          style={{ width: 220 }}
          value={userId}
          onChange={(event) => onUserIdChange(event.target.value)}
          placeholder="user_id"
        />
        <Select
          allowClear
          style={{ width: 220 }}
          placeholder={t('tooling.notepad.folderFilter')}
          value={folderFilter}
          options={folders.map((folder) => ({
            label: folder,
            value: folder,
          }))}
          onChange={onFolderFilterChange}
        />
        <Select
          mode="multiple"
          allowClear
          style={{ minWidth: 260 }}
          placeholder={t('tooling.notepad.tagFilter')}
          value={tagFilter}
          options={tags.map((tag) => ({
            label: `${tag.tag} (${tag.count})`,
            value: tag.tag,
          }))}
          onChange={onTagFilterChange}
        />
        <Input.Search
          allowClear
          style={{ width: 280 }}
          placeholder={t('tooling.notepad.searchPlaceholder')}
          value={queryText}
          onChange={(event) => onQueryTextChange(event.target.value)}
          onSearch={onQueryTextChange}
        />
        <Button onClick={onClearFilters}>{t('common.clearFilters')}</Button>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
      </Space>

      <Space size="large" wrap>
        <Statistic title={t('tooling.notepad.folders')} value={folders.length} />
        <Statistic title={t('tooling.notepad.tags')} value={tags.length} />
        <Statistic title={t('tooling.notepad.notes')} value={notes.length} />
      </Space>

      <Table<ToolingNoteSummary>
        rowKey="id"
        loading={loading}
        columns={columns}
        dataSource={notes}
        pagination={{ pageSize: 10 }}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={t('tooling.notepad.empty')}
            />
          ),
        }}
      />
    </Space>
  );
}

type ToolingTerminalPanelProps = {
  t: TranslateFn;
  userId: string;
  projectId: string;
  includeExited: boolean;
  processes: ToolingTerminalProcessRecord[];
  loading: boolean;
  killLoading: boolean;
  onUserIdChange: (value: string) => void;
  onProjectIdChange: (value: string) => void;
  onIncludeExitedChange: (value: boolean) => void;
  onRefresh: () => void;
  onOpenLogs: (terminalId: string) => void;
  onKill: (terminalId: string) => void;
};

export function ToolingTerminalPanel({
  t,
  userId,
  projectId,
  includeExited,
  processes,
  loading,
  killLoading,
  onUserIdChange,
  onProjectIdChange,
  onIncludeExitedChange,
  onRefresh,
  onOpenLogs,
  onKill,
}: ToolingTerminalPanelProps) {
  const columns = useMemo(
    () => buildToolingTerminalColumns({ t, killLoading, onOpenLogs, onKill }),
    [t, killLoading, onOpenLogs, onKill],
  );
  const runningCount = processes.filter((process) => process.status !== 'exited').length;
  const exitedCount = processes.filter((process) => process.status === 'exited').length;

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space wrap>
        <Input
          allowClear
          style={{ width: 220 }}
          value={userId}
          onChange={(event) => onUserIdChange(event.target.value)}
          placeholder="user_id"
        />
        <Input
          allowClear
          style={{ width: 260 }}
          value={projectId}
          onChange={(event) => onProjectIdChange(event.target.value)}
          placeholder="project_id"
        />
        <Space size={8}>
          <Typography.Text type="secondary">{t('tooling.terminal.includeExited')}</Typography.Text>
          <Switch checked={includeExited} onChange={onIncludeExitedChange} />
        </Space>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
      </Space>

      <Space size="large" wrap>
        <Statistic title={t('tooling.terminal.processes')} value={processes.length} />
        <Statistic title={t('tooling.terminal.running')} value={runningCount} />
        <Statistic title={t('tooling.terminal.exited')} value={exitedCount} />
      </Space>

      <Table<ToolingTerminalProcessRecord>
        rowKey="terminal_id"
        loading={loading}
        columns={columns}
        dataSource={processes}
        pagination={{ pageSize: 10 }}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={t('tooling.terminal.empty')}
            />
          ),
        }}
      />
    </Space>
  );
}

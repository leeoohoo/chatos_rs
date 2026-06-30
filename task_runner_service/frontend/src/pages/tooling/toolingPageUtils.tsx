import { Button, Space, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ToolingNoteSummary, ToolingTerminalProcessRecord } from '../../types';

export type TerminalInputPayload = {
  terminalId: string;
  data: string;
  submit: boolean;
};

export const formatToolingTimestamp = (value: string) => (
  dayjs(value).format('YYYY-MM-DD HH:mm:ss')
);

export const terminalStatusColor = (status: string) => {
  if (status === 'exited') {
    return 'default';
  }
  if (status === 'running') {
    return 'processing';
  }
  return 'warning';
};

export const logKindColor = (kind: string) => {
  if (kind === 'stderr') {
    return 'error';
  }
  if (kind === 'stdout') {
    return 'processing';
  }
  if (kind === 'input') {
    return 'purple';
  }
  if (kind === 'command') {
    return 'cyan';
  }
  return 'default';
};

export const buildToolingNoteColumns = ({
  t,
  onOpenNote,
}: {
  t: TranslateFn;
  onOpenNote: (noteId: string) => void;
}): ColumnsType<ToolingNoteSummary> => [
  {
    title: t('tooling.notepad.titleColumn'),
    dataIndex: 'title',
    render: (_, record) => (
      <Space direction="vertical" size={0}>
        <Typography.Text strong>{record.title}</Typography.Text>
        <Typography.Text type="secondary">{record.folder || '/'}</Typography.Text>
      </Space>
    ),
  },
  {
    title: t('tooling.notepad.tagsColumn'),
    dataIndex: 'tags',
    width: 220,
    render: (tags: string[]) =>
      tags.length ? (
        <Space size={[4, 4]} wrap>
          {tags.map((tag) => (
            <Tag key={tag}>{tag}</Tag>
          ))}
        </Space>
      ) : (
        '-'
      ),
  },
  {
    title: t('tooling.notepad.updatedAt'),
    dataIndex: 'updated_at',
    width: 180,
    render: formatToolingTimestamp,
  },
  {
    title: t('common.actions'),
    key: 'actions',
    width: 120,
    render: (_, record) => (
      <Button size="small" onClick={() => onOpenNote(record.id)}>
        {t('common.view')}
      </Button>
    ),
  },
];

export const buildToolingTerminalColumns = ({
  t,
  killLoading,
  onOpenLogs,
  onKill,
}: {
  t: TranslateFn;
  killLoading: boolean;
  onOpenLogs: (terminalId: string) => void;
  onKill: (terminalId: string) => void;
}): ColumnsType<ToolingTerminalProcessRecord> => [
  {
    title: t('tooling.terminal.terminal'),
    dataIndex: 'terminal_name',
    width: 180,
    render: (_, record) => (
      <Space direction="vertical" size={0}>
        <Typography.Text strong>{record.terminal_name}</Typography.Text>
        <Typography.Text code>{record.terminal_id.slice(0, 12)}</Typography.Text>
      </Space>
    ),
  },
  {
    title: t('common.status'),
    dataIndex: 'status',
    width: 120,
    render: (value: string) => <Tag color={terminalStatusColor(value)}>{value}</Tag>,
  },
  {
    title: t('tooling.terminal.command'),
    dataIndex: 'command',
    render: (value: string) => (
      <Typography.Paragraph ellipsis={{ rows: 2 }} style={{ marginBottom: 0 }}>
        {value}
      </Typography.Paragraph>
    ),
  },
  {
    title: t('tooling.terminal.cwd'),
    dataIndex: 'cwd',
    width: 260,
    ellipsis: true,
  },
  {
    title: t('tooling.terminal.lastActive'),
    dataIndex: 'last_active_at',
    width: 180,
    render: formatToolingTimestamp,
  },
  {
    title: t('common.actions'),
    key: 'actions',
    width: 180,
    render: (_, record) => (
      <Space>
        <Button size="small" onClick={() => onOpenLogs(record.terminal_id)}>
          {t('common.logs')}
        </Button>
        <Button
          size="small"
          danger
          disabled={record.status === 'exited'}
          loading={killLoading}
          onClick={() => onKill(record.terminal_id)}
        >
          {t('tooling.terminal.kill')}
        </Button>
      </Space>
    ),
  },
];

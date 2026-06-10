import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  Input,
  List,
  Select,
  Segmented,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type {
  ToolingNoteSummary,
  ToolingTerminalLogEntry,
  ToolingTerminalProcessRecord,
} from '../types';

const terminalStatusColor = (status: string) => {
  if (status === 'exited') {
    return 'default';
  }
  if (status === 'running') {
    return 'processing';
  }
  return 'warning';
};

const logKindColor = (kind: string) => {
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

export function ToolingPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [view, setView] = useState<'notepad' | 'terminal'>('notepad');
  const [notepadUserId, setNotepadUserId] = useState('task_runner');
  const [folderFilter, setFolderFilter] = useState<string | undefined>(undefined);
  const [tagFilter, setTagFilter] = useState<string[]>([]);
  const [noteQueryText, setNoteQueryText] = useState('');
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [terminalUserId, setTerminalUserId] = useState('');
  const [terminalProjectId, setTerminalProjectId] = useState('');
  const [includeExited, setIncludeExited] = useState(true);
  const [selectedTerminalId, setSelectedTerminalId] = useState<string | null>(null);
  const [terminalInput, setTerminalInput] = useState('');

  const resolvedNotepadUserId = notepadUserId.trim() || 'task_runner';
  const resolvedTerminalUserId = terminalUserId.trim() || undefined;
  const resolvedTerminalProjectId = terminalProjectId.trim() || undefined;

  const notepadFoldersQuery = useQuery({
    queryKey: ['tooling', 'notepad', 'folders', resolvedNotepadUserId],
    queryFn: () => api.listToolingNotepadFolders(resolvedNotepadUserId),
  });
  const notepadTagsQuery = useQuery({
    queryKey: ['tooling', 'notepad', 'tags', resolvedNotepadUserId],
    queryFn: () => api.listToolingNotepadTags(resolvedNotepadUserId),
  });
  const notepadNotesQuery = useQuery({
    queryKey: [
      'tooling',
      'notepad',
      'notes',
      resolvedNotepadUserId,
      folderFilter,
      tagFilter.join(','),
      noteQueryText,
    ],
    queryFn: () =>
      api.listToolingNotepadNotes({
        userId: resolvedNotepadUserId,
        folder: folderFilter,
        tags: tagFilter,
        query: noteQueryText.trim() || undefined,
        limit: 200,
      }),
  });
  const selectedNoteQuery = useQuery({
    queryKey: ['tooling', 'notepad', 'note', resolvedNotepadUserId, selectedNoteId],
    queryFn: () => api.getToolingNotepadNote(selectedNoteId!, resolvedNotepadUserId),
    enabled: Boolean(selectedNoteId),
  });

  const terminalProcessesQuery = useQuery({
    queryKey: [
      'tooling',
      'terminal',
      'processes',
      resolvedTerminalUserId,
      resolvedTerminalProjectId,
      includeExited,
    ],
    queryFn: () =>
      api.listToolingTerminalProcesses({
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
        includeExited,
        limit: 100,
      }),
  });
  const selectedTerminalLogsQuery = useQuery({
    queryKey: [
      'tooling',
      'terminal',
      'logs',
      selectedTerminalId,
      resolvedTerminalUserId,
      resolvedTerminalProjectId,
    ],
    queryFn: () =>
      api.getToolingTerminalProcessLogs(selectedTerminalId!, {
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
        limit: 200,
      }),
    enabled: Boolean(selectedTerminalId),
  });

  const killTerminalMutation = useMutation({
    mutationFn: (terminalId: string) =>
      api.killToolingTerminalProcess(terminalId, {
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['tooling', 'terminal', 'processes'] }),
        queryClient.invalidateQueries({ queryKey: ['tooling', 'terminal', 'logs'] }),
      ]);
      messageApi.success(t('tooling.terminal.killed'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const writeTerminalMutation = useMutation({
    mutationFn: ({
      terminalId,
      data,
      submit,
    }: {
      terminalId: string;
      data: string;
      submit: boolean;
    }) =>
      api.writeToolingTerminalProcess(terminalId, {
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
        data,
        submit,
      }),
    onSuccess: async () => {
      setTerminalInput('');
      await queryClient.invalidateQueries({ queryKey: ['tooling', 'terminal', 'logs'] });
      messageApi.success(t('tooling.terminal.inputSent'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const noteColumns: ColumnsType<ToolingNoteSummary> = [
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
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 120,
      render: (_, record) => (
        <Button size="small" onClick={() => setSelectedNoteId(record.id)}>
          {t('common.view')}
        </Button>
      ),
    },
  ];

  const terminalColumns: ColumnsType<ToolingTerminalProcessRecord> = [
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
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 180,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => setSelectedTerminalId(record.terminal_id)}>
            {t('common.logs')}
          </Button>
          <Button
            size="small"
            danger
            disabled={record.status === 'exited'}
            loading={killTerminalMutation.isPending}
            onClick={() => killTerminalMutation.mutate(record.terminal_id)}
          >
            {t('tooling.terminal.kill')}
          </Button>
        </Space>
      ),
    },
  ];

  const selectedTerminal = useMemo(
    () =>
      (terminalProcessesQuery.data?.processes || []).find(
        (process) => process.terminal_id === selectedTerminalId,
      ) || null,
    [selectedTerminalId, terminalProcessesQuery.data?.processes],
  );

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              {t('tooling.title')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('tooling.subtitle')}
            </Typography.Text>
          </Space>
          <Segmented
            value={view}
            onChange={(value) => setView(value as 'notepad' | 'terminal')}
            options={[
              { label: 'Notepad', value: 'notepad' },
              { label: 'Terminal', value: 'terminal' },
            ]}
          />
        </Space>

        {view === 'notepad' ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Input
                style={{ width: 220 }}
                value={notepadUserId}
                onChange={(event) => setNotepadUserId(event.target.value)}
                placeholder="user_id"
              />
              <Select
                allowClear
                style={{ width: 220 }}
                placeholder={t('tooling.notepad.folderFilter')}
                value={folderFilter}
                options={(notepadFoldersQuery.data?.folders || []).map((folder) => ({
                  label: folder,
                  value: folder,
                }))}
                onChange={setFolderFilter}
              />
              <Select
                mode="multiple"
                allowClear
                style={{ minWidth: 260 }}
                placeholder={t('tooling.notepad.tagFilter')}
                value={tagFilter}
                options={(notepadTagsQuery.data?.tags || []).map((tag) => ({
                  label: `${tag.tag} (${tag.count})`,
                  value: tag.tag,
                }))}
                onChange={setTagFilter}
              />
              <Input.Search
                allowClear
                style={{ width: 280 }}
                placeholder={t('tooling.notepad.searchPlaceholder')}
                value={noteQueryText}
                onChange={(event) => setNoteQueryText(event.target.value)}
                onSearch={(value) => setNoteQueryText(value)}
              />
              <Button
                onClick={() => {
                  setFolderFilter(undefined);
                  setTagFilter([]);
                  setNoteQueryText('');
                }}
              >
                {t('common.clearFilters')}
              </Button>
              <Button
                onClick={() => {
                  void notepadFoldersQuery.refetch();
                  void notepadTagsQuery.refetch();
                  void notepadNotesQuery.refetch();
                }}
              >
                {t('common.refresh')}
              </Button>
            </Space>

            <Space size="large" wrap>
              <Statistic title={t('tooling.notepad.folders')} value={notepadFoldersQuery.data?.folders.length || 0} />
              <Statistic title={t('tooling.notepad.tags')} value={notepadTagsQuery.data?.tags.length || 0} />
              <Statistic title={t('tooling.notepad.notes')} value={notepadNotesQuery.data?.notes.length || 0} />
            </Space>

            <Table<ToolingNoteSummary>
              rowKey="id"
              loading={notepadNotesQuery.isLoading}
              columns={noteColumns}
              dataSource={notepadNotesQuery.data?.notes || []}
              pagination={{ pageSize: 10 }}
              locale={{
                emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.notepad.empty')} />,
              }}
            />
          </Space>
        ) : (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Input
                allowClear
                style={{ width: 220 }}
                value={terminalUserId}
                onChange={(event) => setTerminalUserId(event.target.value)}
                placeholder="user_id"
              />
              <Input
                allowClear
                style={{ width: 260 }}
                value={terminalProjectId}
                onChange={(event) => setTerminalProjectId(event.target.value)}
                placeholder="project_id"
              />
              <Space size={8}>
                <Typography.Text type="secondary">{t('tooling.terminal.includeExited')}</Typography.Text>
                <Switch checked={includeExited} onChange={setIncludeExited} />
              </Space>
              <Button onClick={() => terminalProcessesQuery.refetch()}>{t('common.refresh')}</Button>
            </Space>

            <Space size="large" wrap>
              <Statistic
                title={t('tooling.terminal.processes')}
                value={terminalProcessesQuery.data?.processes.length || 0}
              />
              <Statistic
                title={t('tooling.terminal.running')}
                value={
                  (terminalProcessesQuery.data?.processes || []).filter(
                    (process) => process.status !== 'exited',
                  ).length
                }
              />
              <Statistic
                title={t('tooling.terminal.exited')}
                value={
                  (terminalProcessesQuery.data?.processes || []).filter(
                    (process) => process.status === 'exited',
                  ).length
                }
              />
            </Space>

            <Table<ToolingTerminalProcessRecord>
              rowKey="terminal_id"
              loading={terminalProcessesQuery.isLoading}
              columns={terminalColumns}
              dataSource={terminalProcessesQuery.data?.processes || []}
              pagination={{ pageSize: 10 }}
              locale={{
                emptyText: (
                  <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.terminal.empty')} />
                ),
              }}
            />
          </Space>
        )}
      </Space>

      <Drawer
        title={selectedNoteQuery.data?.note.title || t('tooling.notepad.detailTitle')}
        open={Boolean(selectedNoteId)}
        width={760}
        onClose={() => setSelectedNoteId(null)}
      >
        {selectedNoteQuery.data ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="ID">{selectedNoteQuery.data.note.id}</Descriptions.Item>
              <Descriptions.Item label={t('tooling.notepad.folder')}>
                {selectedNoteQuery.data.note.folder || '/'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tooling.notepad.tags')}>
                {selectedNoteQuery.data.note.tags.length ? (
                  <Space size={[4, 4]} wrap>
                    {selectedNoteQuery.data.note.tags.map((tag) => (
                      <Tag key={tag}>{tag}</Tag>
                    ))}
                  </Space>
                ) : (
                  '-'
                )}
              </Descriptions.Item>
              <Descriptions.Item label={t('tooling.notepad.file')}>{selectedNoteQuery.data.note.file}</Descriptions.Item>
              <Descriptions.Item label={t('tooling.notepad.updatedAt')}>
                {dayjs(selectedNoteQuery.data.note.updated_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
            </Descriptions>

            <Typography.Paragraph
              style={{
                whiteSpace: 'pre-wrap',
                background: '#fafafa',
                padding: 12,
                borderRadius: 6,
                marginBottom: 0,
                fontFamily: 'monospace',
              }}
            >
              {selectedNoteQuery.data.content}
            </Typography.Paragraph>
          </Space>
        ) : selectedNoteQuery.isLoading ? (
          <Typography.Text type="secondary">{t('tooling.notepad.loading')}</Typography.Text>
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.notepad.notFound')} />
        )}
      </Drawer>

      <Drawer
        title={selectedTerminal?.terminal_name || t('tooling.terminal.logTitle')}
        open={Boolean(selectedTerminalId)}
        width={860}
        onClose={() => {
          setSelectedTerminalId(null);
          setTerminalInput('');
        }}
      >
        {selectedTerminalLogsQuery.data ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Button onClick={() => selectedTerminalLogsQuery.refetch()}>{t('tooling.terminal.refreshLogs')}</Button>
              <Button
                danger
                disabled={selectedTerminalLogsQuery.data.status === 'exited'}
                loading={killTerminalMutation.isPending}
                onClick={() => killTerminalMutation.mutate(selectedTerminalLogsQuery.data.terminal_id)}
              >
                {t('tooling.terminal.killProcess')}
              </Button>
            </Space>

            <Space direction="vertical" size={8} style={{ width: '100%' }}>
              <Typography.Text strong>{t('tooling.terminal.sendInput')}</Typography.Text>
              <Input.TextArea
                rows={3}
                value={terminalInput}
                onChange={(event) => setTerminalInput(event.target.value)}
                placeholder={t('tooling.terminal.inputPlaceholder')}
              />
              <Space wrap>
                <Button
                  type="primary"
                  disabled={!terminalInput.trim()}
                  loading={writeTerminalMutation.isPending}
                  onClick={() =>
                    writeTerminalMutation.mutate({
                      terminalId: selectedTerminalLogsQuery.data.terminal_id,
                      data: terminalInput,
                      submit: true,
                    })
                  }
                >
                  {t('tooling.terminal.sendAndEnter')}
                </Button>
                <Button
                  disabled={!terminalInput}
                  loading={writeTerminalMutation.isPending}
                  onClick={() =>
                    writeTerminalMutation.mutate({
                      terminalId: selectedTerminalLogsQuery.data.terminal_id,
                      data: terminalInput,
                      submit: false,
                    })
                  }
                >
                  {t('tooling.terminal.sendOnly')}
                </Button>
              </Space>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="Terminal ID">
                {selectedTerminalLogsQuery.data.terminal_id}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <Tag color={terminalStatusColor(selectedTerminalLogsQuery.data.status)}>
                  {selectedTerminalLogsQuery.data.status}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('tooling.terminal.command')}>
                {selectedTerminalLogsQuery.data.command}
              </Descriptions.Item>
              <Descriptions.Item label={t('tooling.terminal.cwd')}>
                {selectedTerminalLogsQuery.data.cwd}
              </Descriptions.Item>
              <Descriptions.Item label={t('tooling.terminal.lastActive')}>
                {dayjs(selectedTerminalLogsQuery.data.last_active_at).format(
                  'YYYY-MM-DD HH:mm:ss',
                )}
              </Descriptions.Item>
              <Descriptions.Item label="Exit Code">
                {selectedTerminalLogsQuery.data.exit_code ?? '-'}
              </Descriptions.Item>
            </Descriptions>

            {selectedTerminalLogsQuery.data.logs.length ? (
              <List<ToolingTerminalLogEntry>
                bordered
                dataSource={selectedTerminalLogsQuery.data.logs}
                renderItem={(entry) => (
                  <List.Item>
                    <Space
                      direction="vertical"
                      size={4}
                      style={{ width: '100%', alignItems: 'flex-start' }}
                    >
                      <Space wrap>
                        <Tag color={logKindColor(entry.kind)}>{entry.kind}</Tag>
                        <Typography.Text type="secondary">
                          offset: {entry.offset}
                        </Typography.Text>
                        <Typography.Text type="secondary">
                          {dayjs(entry.created_at).format('YYYY-MM-DD HH:mm:ss')}
                        </Typography.Text>
                      </Space>
                      <Typography.Paragraph
                        style={{
                          marginBottom: 0,
                          whiteSpace: 'pre-wrap',
                          fontFamily: 'monospace',
                        }}
                      >
                        {entry.content}
                      </Typography.Paragraph>
                    </Space>
                  </List.Item>
                )}
              />
            ) : (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.terminal.noLogs')} />
            )}
          </Space>
        ) : selectedTerminalLogsQuery.isLoading ? (
          <Typography.Text type="secondary">{t('tooling.terminal.loadingLogs')}</Typography.Text>
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.terminal.logsNotFound')} />
        )}
      </Drawer>
    </>
  );
}

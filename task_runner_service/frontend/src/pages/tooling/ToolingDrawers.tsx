import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  Input,
  List,
  Space,
  Tag,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  ToolingNotepadNoteResponse,
  ToolingTerminalLogEntry,
  ToolingTerminalProcessLogsResponse,
} from '../../types';
import {
  formatToolingTimestamp,
  logKindColor,
  terminalStatusColor,
  type TerminalInputPayload,
} from './toolingPageUtils';

type ToolingNoteDetailDrawerProps = {
  t: TranslateFn;
  open: boolean;
  data?: ToolingNotepadNoteResponse;
  loading: boolean;
  onClose: () => void;
};

export function ToolingNoteDetailDrawer({
  t,
  open,
  data,
  loading,
  onClose,
}: ToolingNoteDetailDrawerProps) {
  return (
    <Drawer
      title={data?.note.title || t('tooling.notepad.detailTitle')}
      open={open}
      width={760}
      onClose={onClose}
    >
      {data ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label="ID">{data.note.id}</Descriptions.Item>
            <Descriptions.Item label={t('tooling.notepad.folder')}>
              {data.note.folder || '/'}
            </Descriptions.Item>
            <Descriptions.Item label={t('tooling.notepad.tags')}>
              {data.note.tags.length ? (
                <Space size={[4, 4]} wrap>
                  {data.note.tags.map((tag) => (
                    <Tag key={tag}>{tag}</Tag>
                  ))}
                </Space>
              ) : (
                '-'
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tooling.notepad.file')}>
              {data.note.file}
            </Descriptions.Item>
            <Descriptions.Item label={t('tooling.notepad.updatedAt')}>
              {formatToolingTimestamp(data.note.updated_at)}
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
            {data.content}
          </Typography.Paragraph>
        </Space>
      ) : loading ? (
        <Typography.Text type="secondary">{t('tooling.notepad.loading')}</Typography.Text>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.notepad.notFound')} />
      )}
    </Drawer>
  );
}

type ToolingTerminalLogDrawerProps = {
  t: TranslateFn;
  open: boolean;
  title: string;
  data?: ToolingTerminalProcessLogsResponse;
  loading: boolean;
  input: string;
  killLoading: boolean;
  writeLoading: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onInputChange: (value: string) => void;
  onKill: (terminalId: string) => void;
  onSendInput: (payload: TerminalInputPayload) => void;
};

export function ToolingTerminalLogDrawer({
  t,
  open,
  title,
  data,
  loading,
  input,
  killLoading,
  writeLoading,
  onClose,
  onRefresh,
  onInputChange,
  onKill,
  onSendInput,
}: ToolingTerminalLogDrawerProps) {
  return (
    <Drawer title={title} open={open} width={860} onClose={onClose}>
      {data ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space wrap>
            <Button onClick={onRefresh}>{t('tooling.terminal.refreshLogs')}</Button>
            <Button
              danger
              disabled={data.status === 'exited'}
              loading={killLoading}
              onClick={() => onKill(data.terminal_id)}
            >
              {t('tooling.terminal.killProcess')}
            </Button>
          </Space>

          <Space direction="vertical" size={8} style={{ width: '100%' }}>
            <Typography.Text strong>{t('tooling.terminal.sendInput')}</Typography.Text>
            <Input.TextArea
              rows={3}
              value={input}
              onChange={(event) => onInputChange(event.target.value)}
              placeholder={t('tooling.terminal.inputPlaceholder')}
            />
            <Space wrap>
              <Button
                type="primary"
                disabled={!input.trim()}
                loading={writeLoading}
                onClick={() =>
                  onSendInput({
                    terminalId: data.terminal_id,
                    data: input,
                    submit: true,
                  })
                }
              >
                {t('tooling.terminal.sendAndEnter')}
              </Button>
              <Button
                disabled={!input}
                loading={writeLoading}
                onClick={() =>
                  onSendInput({
                    terminalId: data.terminal_id,
                    data: input,
                    submit: false,
                  })
                }
              >
                {t('tooling.terminal.sendOnly')}
              </Button>
            </Space>
          </Space>

          <TerminalLogDescriptions t={t} data={data} />
          <TerminalLogList t={t} logs={data.logs} />
        </Space>
      ) : loading ? (
        <Typography.Text type="secondary">{t('tooling.terminal.loadingLogs')}</Typography.Text>
      ) : (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={t('tooling.terminal.logsNotFound')}
        />
      )}
    </Drawer>
  );
}

function TerminalLogDescriptions({
  t,
  data,
}: {
  t: TranslateFn;
  data: ToolingTerminalProcessLogsResponse;
}) {
  return (
    <Descriptions bordered column={1} size="small">
      <Descriptions.Item label="Terminal ID">{data.terminal_id}</Descriptions.Item>
      <Descriptions.Item label={t('common.status')}>
        <Tag color={terminalStatusColor(data.status)}>{data.status}</Tag>
      </Descriptions.Item>
      <Descriptions.Item label={t('tooling.terminal.command')}>
        {data.command}
      </Descriptions.Item>
      <Descriptions.Item label={t('tooling.terminal.cwd')}>
        {data.cwd}
      </Descriptions.Item>
      <Descriptions.Item label={t('tooling.terminal.lastActive')}>
        {formatToolingTimestamp(data.last_active_at)}
      </Descriptions.Item>
      <Descriptions.Item label="Exit Code">{data.exit_code ?? '-'}</Descriptions.Item>
    </Descriptions>
  );
}

function TerminalLogList({
  t,
  logs,
}: {
  t: TranslateFn;
  logs: ToolingTerminalLogEntry[];
}) {
  if (!logs.length) {
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tooling.terminal.noLogs')} />;
  }

  return (
    <List<ToolingTerminalLogEntry>
      bordered
      dataSource={logs}
      renderItem={(entry) => (
        <List.Item>
          <Space
            direction="vertical"
            size={4}
            style={{ width: '100%', alignItems: 'flex-start' }}
          >
            <Space wrap>
              <Tag color={logKindColor(entry.kind)}>{entry.kind}</Tag>
              <Typography.Text type="secondary">offset: {entry.offset}</Typography.Text>
              <Typography.Text type="secondary">
                {formatToolingTimestamp(entry.created_at)}
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
  );
}

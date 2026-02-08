import React, { useEffect, useMemo, useState } from 'react';
import {
  Layout,
  Typography,
  Space,
  Button,
  Table,
  Input,
  Select,
  Drawer,
  Card,
  Descriptions,
  Tag,
  InputNumber,
  message
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { ReloadOutlined } from '@ant-design/icons';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism';
import { apiGet } from './api';
import type { ChangeRecord, FileResponse, StatusResponse } from './types';

const { Header, Content } = Layout;
const { Title, Text } = Typography;

function formatDate(value?: string) {
  if (!value) return '';
  return value.replace('T', ' ').replace('Z', '');
}

function guessLanguage(path: string) {
  const ext = path.split('.').pop()?.toLowerCase() || '';
  const map: Record<string, string> = {
    rs: 'rust',
    ts: 'typescript',
    tsx: 'tsx',
    js: 'javascript',
    jsx: 'jsx',
    json: 'json',
    md: 'markdown',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    css: 'css',
    html: 'html',
    sh: 'bash',
    py: 'python',
    go: 'go',
    java: 'java',
    c: 'c',
    h: 'c',
    cpp: 'cpp',
    hpp: 'cpp',
    txt: 'text'
  };
  return map[ext] || 'text';
}

function actionColor(action: string) {
  if (action === 'delete') return 'red';
  if (action === 'append') return 'blue';
  if (action === 'write') return 'green';
  return 'default';
}

export default function App() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);

  const [changes, setChanges] = useState<ChangeRecord[]>([]);
  const [changesLoading, setChangesLoading] = useState(false);

  const [sessionFilter, setSessionFilter] = useState('');
  const [runFilter, setRunFilter] = useState('');
  const [dirFilter, setDirFilter] = useState('');
  const [actionFilter, setActionFilter] = useState('');
  const [limit, setLimit] = useState(200);

  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [fileDetail, setFileDetail] = useState<FileResponse | null>(null);
  const [fileChanges, setFileChanges] = useState<ChangeRecord[]>([]);
  const [fileLoading, setFileLoading] = useState(false);

  const refreshStatus = async () => {
    setStatusLoading(true);
    try {
      const data = await apiGet<StatusResponse>('/api/status');
      setStatus(data);
    } catch (err) {
      message.error(String(err));
    } finally {
      setStatusLoading(false);
    }
  };

  const refreshChanges = async () => {
    setChangesLoading(true);
    try {
      const params = new URLSearchParams();
      if (sessionFilter) params.set('session_id', sessionFilter);
      if (runFilter) params.set('run_id', runFilter);
      if (dirFilter) params.set('path_prefix', dirFilter);
      if (actionFilter) params.set('action', actionFilter);
      if (limit) params.set('limit', String(limit));
      const url = params.toString() ? `/api/changes?${params.toString()}` : '/api/changes';
      const data = await apiGet<{ changes: ChangeRecord[] }>(url);
      setChanges(Array.isArray(data.changes) ? data.changes : []);
    } catch (err) {
      message.error(String(err));
    } finally {
      setChangesLoading(false);
    }
  };

  const openFile = async (path: string) => {
    setSelectedPath(path);
    setFileLoading(true);
    try {
      const detail = await apiGet<FileResponse>(`/api/file?path=${encodeURIComponent(path)}`);
      setFileDetail(detail);
      const data = await apiGet<{ changes: ChangeRecord[] }>(
        `/api/changes?path=${encodeURIComponent(path)}&limit=200&include_diff=1`
      );
      setFileChanges(Array.isArray(data.changes) ? data.changes : []);
    } catch (err) {
      message.error(String(err));
      setFileDetail(null);
      setFileChanges([]);
    } finally {
      setFileLoading(false);
    }
  };

  useEffect(() => {
    refreshStatus();
    refreshChanges();
  }, []);

  const changeColumns: ColumnsType<ChangeRecord> = [
    { title: 'Time', dataIndex: 'created_at', key: 'created_at', render: (value) => formatDate(value) },
    { title: 'Path', dataIndex: 'path', key: 'path' },
    {
      title: 'Action',
      dataIndex: 'action',
      key: 'action',
      render: (value) => <Tag color={actionColor(value)}>{value}</Tag>
    },
    { title: 'Bytes', dataIndex: 'bytes', key: 'bytes' },
    { title: 'Session', dataIndex: 'session_id', key: 'session_id' },
    { title: 'Run', dataIndex: 'run_id', key: 'run_id' },
    { title: 'SHA256', dataIndex: 'sha256', key: 'sha256' }
  ];

  const fileChangeColumns: ColumnsType<ChangeRecord> = [
    { title: 'Time', dataIndex: 'created_at', key: 'created_at', render: (value) => formatDate(value) },
    {
      title: 'Action',
      dataIndex: 'action',
      key: 'action',
      render: (value) => <Tag color={actionColor(value)}>{value}</Tag>
    },
    { title: 'Bytes', dataIndex: 'bytes', key: 'bytes' },
    { title: 'Session', dataIndex: 'session_id', key: 'session_id' },
    { title: 'Run', dataIndex: 'run_id', key: 'run_id' },
    { title: 'SHA256', dataIndex: 'sha256', key: 'sha256' },
    { title: 'Diff', dataIndex: 'diff', key: 'diff', render: (value) => value ? <Tag color="geekblue">diff</Tag> : <Text type="secondary">-</Text> }
  ];

  const language = useMemo(() => (fileDetail ? guessLanguage(fileDetail.path) : 'text'), [fileDetail]);

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Header style={{ background: '#0f172a', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <Title level={4} style={{ color: 'white', margin: 0 }}>Code Maintainer Admin</Title>
        <Space>
          <Button icon={<ReloadOutlined />} onClick={refreshStatus} loading={statusLoading}>Refresh Status</Button>
        </Space>
      </Header>
      <Content style={{ padding: 24 }}>
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Card size="small" title="Status">
            <Descriptions
              size="small"
              bordered
              column={2}
              items={[
                { key: 'server', label: 'Server', children: status?.server_name || '-' },
                { key: 'root', label: 'Root', children: status?.root || '-' },
                { key: 'db', label: 'DB', children: status?.db_path || '-' },
                { key: 'writes', label: 'Writes', children: status?.allow_writes ? 'Enabled' : 'Disabled' },
                { key: 'session', label: 'Session', children: status?.session_id || '-' },
                { key: 'run', label: 'Run', children: status?.run_id || '-' }
              ]}
            />
          </Card>

          <Card size="small" title="Change Log">
            <Space direction="vertical" size="middle" style={{ width: '100%' }}>
              <Space wrap>
                <Input
                  placeholder="Session ID"
                  value={sessionFilter}
                  onChange={(e) => setSessionFilter(e.target.value)}
                  style={{ width: 180 }}
                />
                <Input
                  placeholder="Run ID"
                  value={runFilter}
                  onChange={(e) => setRunFilter(e.target.value)}
                  style={{ width: 180 }}
                />
                <Input
                  placeholder="Directory prefix (e.g. src/)"
                  value={dirFilter}
                  onChange={(e) => setDirFilter(e.target.value)}
                  style={{ width: 220 }}
                />
                <Select
                  value={actionFilter}
                  onChange={(value) => setActionFilter(value)}
                  style={{ width: 140 }}
                  options={[
                    { value: '', label: 'All Actions' },
                    { value: 'write', label: 'write' },
                    { value: 'append', label: 'append' },
                    { value: 'delete', label: 'delete' }
                  ]}
                />
                <Space>
                  <Text>Limit</Text>
                  <InputNumber min={1} max={1000} value={limit} onChange={(value) => setLimit(value || 200)} />
                </Space>
                <Button onClick={refreshChanges} loading={changesLoading}>Refresh</Button>
              </Space>
              <Table
                rowKey="id"
                dataSource={changes}
                columns={changeColumns}
                loading={changesLoading}
                onRow={(record) => ({
                  onClick: () => openFile(record.path)
                })}
                pagination={{ pageSize: 50 }}
              />
            </Space>
          </Card>
        </Space>
      </Content>

      <Drawer
        title="File Details"
        width={980}
        open={!!selectedPath}
        onClose={() => {
          setSelectedPath(null);
          setFileDetail(null);
          setFileChanges([]);
        }}
      >
        {selectedPath ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Card size="small" title={selectedPath}>
              <Descriptions
                size="small"
                bordered
                column={2}
                items={[
                  { key: 'path', label: 'Path', children: fileDetail?.path || selectedPath },
                  { key: 'size', label: 'Size', children: fileDetail ? `${fileDetail.size_bytes} bytes` : '-' },
                  { key: 'sha', label: 'SHA256', children: fileDetail?.sha256 || '-' }
                ]}
              />
              <div style={{ marginTop: 16 }}>
                {fileLoading ? (
                  <Text type="secondary">Loading file...</Text>
                ) : fileDetail ? (
                  <SyntaxHighlighter
                    language={language}
                    style={oneDark}
                    showLineNumbers
                    wrapLongLines
                  >
                    {fileDetail.content}
                  </SyntaxHighlighter>
                ) : (
                  <Text type="secondary">No file data.</Text>
                )}
              </div>
            </Card>
            <Card size="small" title="Change History">
              <Table
                rowKey="id"
                dataSource={fileChanges}
                columns={fileChangeColumns}
                pagination={false}
                expandable={{
                  expandedRowRender: (record) => (
                    record.diff ? (
                      <SyntaxHighlighter
                        language="diff"
                        style={oneDark}
                        wrapLongLines
                      >
                        {record.diff}
                      </SyntaxHighlighter>
                    ) : (
                      <Text type="secondary">No diff stored for this change.</Text>
                    )
                  )
                }}
              />
            </Card>
          </Space>
        ) : null}
      </Drawer>
    </Layout>
  );
}

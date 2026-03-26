import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Collapse,
  Empty,
  Input,
  Modal,
  Select,
  Space,
  Spin,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { MemorySkill, MemorySkillPlugin } from '../types';

const { Text } = Typography;

interface SkillsPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
}

export function SkillsPage({ filterUserId, currentUserId, isAdmin }: SkillsPageProps) {
  const { t } = useI18n();
  const [plugins, setPlugins] = useState<MemorySkillPlugin[]>([]);
  const [skills, setSkills] = useState<MemorySkill[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [pluginFilter, setPluginFilter] = useState<string | undefined>(undefined);
  const [importOpen, setImportOpen] = useState(false);
  const [repository, setRepository] = useState('');
  const [branch, setBranch] = useState('');
  const [autoInstall, setAutoInstall] = useState(true);
  const [pluginDetailOpen, setPluginDetailOpen] = useState(false);
  const [pluginDetailLoading, setPluginDetailLoading] = useState(false);
  const [pluginDetail, setPluginDetail] = useState<MemorySkillPlugin | null>(null);
  const [pluginDetailSkills, setPluginDetailSkills] = useState<MemorySkill[]>([]);
  const [skillDetailOpen, setSkillDetailOpen] = useState(false);
  const [skillDetailLoading, setSkillDetailLoading] = useState(false);
  const [skillDetail, setSkillDetail] = useState<MemorySkill | null>(null);

  const scopeUserId = useMemo(() => {
    if (!isAdmin) {
      return currentUserId.trim();
    }
    const filtered = filterUserId?.trim();
    return filtered && filtered.length > 0 ? filtered : currentUserId.trim();
  }, [isAdmin, filterUserId, currentUserId]);

  const loadPlugins = async () => {
    const items = await api.listSkillPlugins(scopeUserId, { limit: 300, offset: 0 });
    setPlugins(items);
  };

  const loadSkills = async () => {
    const items = await api.listSkills(scopeUserId, {
      plugin_source: pluginFilter,
      query: query.trim() || undefined,
      limit: 500,
      offset: 0,
    });
    setSkills(items);
  };

  const loadAll = async () => {
    setLoading(true);
    setError(null);
    try {
      await Promise.all([loadPlugins(), loadSkills()]);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeUserId, pluginFilter]);

  const searchSkills = async () => {
    setLoading(true);
    setError(null);
    try {
      await loadSkills();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  const runImport = async () => {
    const targetRepository = repository.trim();
    if (!targetRepository) {
      setError(t('skills.repositoryRequired'));
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await api.importSkillsFromGit({
        user_id: scopeUserId,
        repository: targetRepository,
        branch: branch.trim() || undefined,
        auto_install: autoInstall,
      });
      setImportOpen(false);
      setRepository('');
      setBranch('');
      setAutoInstall(true);
      await loadAll();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const installPlugin = async (source?: string) => {
    setSaving(true);
    setError(null);
    try {
      await api.installSkillPlugins({
        user_id: scopeUserId,
        source,
        install_all: !source,
      });
      await loadAll();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const loadAllPluginSkills = async (pluginSource: string): Promise<MemorySkill[]> => {
    const normalizedPluginSource = pluginSource.trim();
    if (!normalizedPluginSource) {
      return [];
    }

    const pageSize = 500;
    let offset = 0;
    const rows: MemorySkill[] = [];

    while (true) {
      const pageRows = await api.listSkills(scopeUserId, {
        plugin_source: normalizedPluginSource,
        limit: pageSize,
        offset,
      });
      if (pageRows.length === 0) {
        break;
      }
      rows.push(...pageRows);
      if (pageRows.length < pageSize) {
        break;
      }
      offset += pageRows.length;
    }

    return rows;
  };

  const openPluginDetail = async (record: MemorySkillPlugin) => {
    setPluginDetailOpen(true);
    setPluginDetailLoading(true);
    setPluginDetail(record);
    setPluginDetailSkills([]);
    try {
      const [detail, detailSkills] = await Promise.all([
        api.getSkillPlugin(record.source, scopeUserId).catch(() => record),
        loadAllPluginSkills(record.source),
      ]);
      const resolved = detail || record;
      setPluginDetail(resolved);
      setPluginDetailSkills(detailSkills);
      const resolvedCommandCount = resolved.command_count
        ?? resolved.commands?.length
        ?? 0;
      setPlugins((prev) => prev.map((item) => (
        item.source === record.source
          ? { ...item, command_count: resolvedCommandCount }
          : item
      )));
    } catch (err) {
      setPluginDetailOpen(false);
      setError((err as Error).message);
    } finally {
      setPluginDetailLoading(false);
    }
  };

  const openSkillDetail = async (record: MemorySkill) => {
    setSkillDetailOpen(true);
    setSkillDetailLoading(true);
    setSkillDetail(record);
    try {
      const detail = await api.getSkill(record.id, scopeUserId);
      setSkillDetail(detail || record);
    } catch {
      setSkillDetail(record);
    } finally {
      setSkillDetailLoading(false);
    }
  };

  const pluginColumns: ColumnsType<MemorySkillPlugin> = [
    {
      title: t('skills.pluginName'),
      dataIndex: 'name',
      key: 'name',
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0, height: 20, fontWeight: 600 }}
            onClick={() => {
              void openPluginDetail(record);
            }}
          >
            {value || '-'}
          </Button>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {record.source}
          </Text>
        </Space>
      ),
    },
    {
      title: t('skills.status'),
      dataIndex: 'installed',
      key: 'installed',
      width: 140,
      render: (value: boolean) => (
        <Tag color={value ? 'green' : 'default'}>{value ? t('skills.installed') : t('skills.notInstalled')}</Tag>
      ),
    },
    {
      title: t('skills.discoverable'),
      dataIndex: 'discoverable_skills',
      key: 'discoverable_skills',
      width: 120,
    },
    {
      title: t('skills.installedCount'),
      dataIndex: 'installed_skill_count',
      key: 'installed_skill_count',
      width: 120,
    },
    {
      title: t('skills.commands'),
      dataIndex: 'command_count',
      key: 'command_count',
      width: 120,
      render: (_: number | undefined, record) => (
        record.command_count ?? record.commands?.length ?? 0
      ),
    },
    {
      title: t('skills.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    {
      title: t('common.action'),
      key: 'action',
      width: 130,
      render: (_, record) => (
        <Button size="small" loading={saving} onClick={() => installPlugin(record.source)}>
          {t('skills.installNow')}
        </Button>
      ),
    },
  ];

  const skillColumns: ColumnsType<MemorySkill> = [
    {
      title: t('skills.skillName'),
      dataIndex: 'name',
      key: 'name',
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0, height: 20, fontWeight: 600 }}
            onClick={() => {
              void openSkillDetail(record);
            }}
          >
            {value}
          </Button>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {record.plugin_source}
          </Text>
        </Space>
      ),
    },
    {
      title: t('skills.path'),
      dataIndex: 'source_path',
      key: 'source_path',
      width: 280,
      render: (value: string) => <Text code>{value}</Text>,
    },
    {
      title: t('skills.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
  ];

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <Card
        title={t('skills.title')}
        extra={
          <Space>
            <Button onClick={loadAll} loading={loading}>
              {t('common.refresh')}
            </Button>
            <Button onClick={() => installPlugin(undefined)} loading={saving}>
              {t('skills.installAll')}
            </Button>
            <Button type="primary" onClick={() => setImportOpen(true)}>
              {t('skills.importGit')}
            </Button>
          </Space>
        }
      >
        {isAdmin && !filterUserId?.trim() && (
          <Alert type="info" showIcon message={t('skills.adminTip')} style={{ marginBottom: 12 }} />
        )}
        <Alert
          type="info"
          showIcon
          message={`${t('skills.scopeUser')}: ${scopeUserId || '-'}`}
          style={{ marginBottom: 12 }}
        />
        {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}

        <Table
          rowKey="id"
          loading={loading}
          columns={pluginColumns}
          dataSource={plugins}
          pagination={{ pageSize: 10, showSizeChanger: false }}
          size="middle"
        />
      </Card>

      <Card title={t('skills.skillList')}>
        <Space style={{ marginBottom: 12 }} wrap>
          <Select
            allowClear
            style={{ width: 320 }}
            placeholder={t('skills.pluginFilter')}
            value={pluginFilter}
            onChange={(value) => setPluginFilter(value)}
            options={plugins.map((item) => ({
              value: item.source,
              label: `${item.name} (${item.source})`,
            }))}
          />
          <Input
            style={{ width: 320 }}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t('skills.search')}
            onPressEnter={searchSkills}
          />
          <Button onClick={searchSkills} loading={loading}>
            {t('common.refresh')}
          </Button>
        </Space>

        <Table
          rowKey="id"
          loading={loading}
          columns={skillColumns}
          dataSource={skills}
          pagination={{ pageSize: 12, showSizeChanger: false }}
          size="middle"
        />
      </Card>

      <Modal
        open={pluginDetailOpen}
        title={`${t('skills.pluginDetail')}: ${pluginDetail?.name || pluginDetail?.source || '-'}`}
        footer={null}
        onCancel={() => {
          setPluginDetailOpen(false);
          setPluginDetail(null);
          setPluginDetailSkills([]);
        }}
        width={920}
      >
        {pluginDetailLoading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
            <Spin />
          </div>
        ) : !pluginDetail ? (
          <Empty description={t('skills.pluginDetail')} />
        ) : (
          <Space direction="vertical" size={10} style={{ width: '100%' }}>
            <Text strong>{pluginDetail.source}</Text>
            <Text type="secondary">
              {t('skills.pluginName')}: {pluginDetail.name || '-'}
            </Text>
            <Text type="secondary">
              {t('skills.status')}: {pluginDetail.installed ? t('skills.installed') : t('skills.notInstalled')}
            </Text>
            <Text type="secondary">
              {t('skills.updatedAt')}: {new Date(pluginDetail.updated_at).toLocaleString()}
            </Text>
            <Text strong>{t('skills.pluginMainContent')}</Text>
            <div
              style={{
                maxHeight: 260,
                overflow: 'auto',
                padding: 12,
                border: '1px solid #f0f0f0',
                borderRadius: 8,
                background: '#fafafa',
              }}
            >
              <pre
                style={{
                  margin: 0,
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  fontFamily: 'SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace',
                  fontSize: 13,
                  lineHeight: 1.6,
                }}
              >
                {pluginDetail.content?.trim()
                  || pluginDetail.description?.trim()
                  || t('skills.pluginMainContentEmpty')}
              </pre>
            </div>
            <Text strong>{t('skills.pluginCommands')}</Text>
            {(pluginDetail.commands || []).length === 0 ? (
              <Empty description={t('skills.pluginNoCommands')} />
            ) : (
              <Collapse
                size="small"
                items={(pluginDetail.commands || []).map((command, index) => ({
                  key: `${command.source_path || command.name || index}`,
                  label: `${command.name || '-'} (${command.source_path || '-'})`,
                  children: (
                    <div
                      style={{
                        maxHeight: 260,
                        overflow: 'auto',
                        padding: 10,
                        border: '1px solid #f0f0f0',
                        borderRadius: 8,
                        background: '#fafafa',
                      }}
                    >
                      <pre
                        style={{
                          margin: 0,
                          whiteSpace: 'pre-wrap',
                          wordBreak: 'break-word',
                          fontFamily: 'SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace',
                          fontSize: 13,
                          lineHeight: 1.6,
                        }}
                      >
                        {command.content || t('skills.pluginCommandContentEmpty')}
                      </pre>
                    </div>
                  ),
                }))}
              />
            )}
            <Text strong>{t('skills.pluginSkills')}</Text>
            {pluginDetailSkills.length === 0 ? (
              <Empty description={t('skills.pluginNoSkills')} />
            ) : (
              <Collapse
                size="small"
                items={pluginDetailSkills.map((item) => ({
                  key: item.id,
                  label: `${item.name} (${item.id})`,
                  children: (
                    <Space direction="vertical" size={8} style={{ width: '100%' }}>
                      <Text type="secondary">
                        {t('skills.path')}: {item.source_path || '-'}
                      </Text>
                      <div
                        style={{
                          maxHeight: 280,
                          overflow: 'auto',
                          padding: 10,
                          border: '1px solid #f0f0f0',
                          borderRadius: 8,
                          background: '#fafafa',
                        }}
                      >
                        <pre
                          style={{
                            margin: 0,
                            whiteSpace: 'pre-wrap',
                            wordBreak: 'break-word',
                            fontFamily: 'SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace',
                            fontSize: 13,
                            lineHeight: 1.6,
                          }}
                        >
                          {item.content || t('skills.skillContentEmpty')}
                        </pre>
                      </div>
                    </Space>
                  ),
                }))}
              />
            )}
          </Space>
        )}
      </Modal>

      <Modal
        open={skillDetailOpen}
        title={`${t('skills.skillDetail')}: ${skillDetail?.name || skillDetail?.id || '-'}`}
        footer={null}
        onCancel={() => {
          setSkillDetailOpen(false);
          setSkillDetail(null);
        }}
        width={860}
      >
        {skillDetailLoading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
            <Spin />
          </div>
        ) : !skillDetail ? (
          <Empty description={t('skills.skillNotFound')} />
        ) : (
          <Space direction="vertical" size={10} style={{ width: '100%' }}>
            <Text strong>{skillDetail.id}</Text>
            <Text type="secondary">{t('skills.pluginName')}: {skillDetail.plugin_source || '-'}</Text>
            <Text type="secondary">{t('skills.path')}: {skillDetail.source_path || '-'}</Text>
            <div
              style={{
                maxHeight: 520,
                overflow: 'auto',
                padding: 12,
                border: '1px solid #f0f0f0',
                borderRadius: 8,
                background: '#fafafa',
              }}
            >
              <pre
                style={{
                  margin: 0,
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  fontFamily: 'SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace',
                  fontSize: 13,
                  lineHeight: 1.6,
                }}
              >
                {skillDetail.content || t('skills.skillContentEmpty')}
              </pre>
            </div>
          </Space>
        )}
      </Modal>

      <Modal
        open={importOpen}
        title={t('skills.importGit')}
        onCancel={() => setImportOpen(false)}
        onOk={runImport}
        confirmLoading={saving}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Input
            value={repository}
            onChange={(event) => setRepository(event.target.value)}
            placeholder={t('skills.repository')}
          />
          <Input
            value={branch}
            onChange={(event) => setBranch(event.target.value)}
            placeholder={t('skills.branch')}
          />
          <Space>
            <Text>{t('skills.autoInstall')}</Text>
            <Switch checked={autoInstall} onChange={setAutoInstall} />
          </Space>
        </Space>
      </Modal>
    </Space>
  );
}

import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Input,
  Modal,
  Select,
  Space,
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

  const pluginColumns: ColumnsType<MemorySkillPlugin> = [
    {
      title: t('skills.pluginName'),
      dataIndex: 'name',
      key: 'name',
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{value || '-'}</Text>
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
          <Text strong>{value}</Text>
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


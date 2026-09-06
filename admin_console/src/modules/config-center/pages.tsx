import { useEffect, useMemo, useState } from 'react';
import {
  App as AntdApp,
  Button,
  Card,
  Col,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Row,
  Select,
  Space,
  Spin,
  Switch,
  Tabs,
  Tag,
  Typography,
} from 'antd';
import {
  PlusOutlined,
  SaveOutlined,
} from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from './api';
import type {
  ConfigDefinition,
  ConfigValue,
} from './types';

const CONFIG_AREA_META: Record<string, { label: string; order: number }> = {
  'chatos-backend': { label: 'Chat OS', order: 10 },
  'task-runner': { label: 'Task Runner', order: 20 },
  'mcp-management-service': { label: 'MCP 管理', order: 30 },
  'memory-engine': { label: 'Memory Engine', order: 40 },
  'project-service': { label: '项目服务', order: 50 },
  'user-service': { label: '用户服务', order: 70 },
  'plugin-management-service': { label: '插件管理', order: 80 },
  'local-connector-service': { label: '本地连接器', order: 90 },
  'configuration-center': { label: '配置中心', order: 100 },
  'official-website': { label: '官方网站', order: 110 },
  'platform-shared': { label: '平台与共享', order: 120 },
  developer: { label: '开发参数', order: 900 },
};

function configAreaKey(definition: ConfigDefinition) {
  const serviceName = definition.service_name?.trim();
  if (serviceName) {
    return serviceName;
  }
  const categoryRoot = definition.category.split('/')[0]?.trim().toLowerCase();
  if (categoryRoot === 'chat os') return 'chatos-backend';
  if (categoryRoot === 'task runner') return 'task-runner';
  if (categoryRoot === 'mcp management') return 'mcp-management-service';
  if (categoryRoot === 'memory engine') return 'memory-engine';
  if (categoryRoot === 'project service') return 'project-service';
  if (categoryRoot === 'user service') return 'user-service';
  if (categoryRoot === 'plugin management') return 'plugin-management-service';
  if (categoryRoot === 'local connector') return 'local-connector-service';
  if (categoryRoot === 'configuration center') return 'configuration-center';
  if (categoryRoot === 'developer') return 'developer';
  return 'platform-shared';
}

function configAreaLabel(key: string) {
  return CONFIG_AREA_META[key]?.label || key;
}

function configCategoryLabel(category: string) {
  const [, ...rest] = category.split('/').map((part) => part.trim());
  return rest.length > 0 ? rest.join(' / ') : category;
}

export function ConfigEditor({ environment }: { environment: string }) {
  const { message, modal } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const catalog = useQuery({ queryKey: ['config-center', 'catalog'], queryFn: api.catalog });
  const effective = useQuery({
    queryKey: ['config-center', 'effective', environment],
    queryFn: () => api.effective(environment),
  });
  const draft = useQuery({
    queryKey: ['config-center', 'draft', environment],
    queryFn: () => api.draft(environment),
  });
  const [changes, setChanges] = useState<Record<string, ConfigValue>>({});
  const [publishMessage, setPublishMessage] = useState('');
  const [customOpen, setCustomOpen] = useState(false);
  const [activeArea, setActiveArea] = useState(
    localStorage.getItem('chatos.configuration-center.config-area') || 'chatos-backend',
  );
  const [configSearch, setConfigSearch] = useState('');
  const [customForm] = Form.useForm<{
    key: string;
    display_name: string;
    service_name: string;
    value_type: string;
    default_value: string;
    reload_mode: string;
    env_alias: string;
  }>();

  useEffect(() => {
    setChanges(draft.data?.draft?.changes || {});
  }, [draft.data?.draft?.id, draft.data?.draft?.updated_at]);

  const save = useMutation({
    mutationFn: () => api.saveDraft(environment, changes),
    onSuccess: async () => {
      message.success('草稿已保存');
      await queryClient.invalidateQueries({ queryKey: ['config-center', 'draft', environment] });
    },
    onError: (error: Error) => message.error(error.message),
  });
  const publish = useMutation({
    mutationFn: async () => {
      await api.saveDraft(environment, changes);
      const validation = await api.validateDraft(environment);
      if (!validation.valid) {
        throw new Error(validation.errors.join('；'));
      }
      return api.publishDraft(environment, publishMessage || '更新平台配置');
    },
    onSuccess: async (release) => {
      message.success(`Revision ${release.revision} 发布成功`);
      setPublishMessage('');
      setChanges({});
      await queryClient.invalidateQueries();
    },
    onError: (error: Error) => message.error(error.message),
  });
  const createCustom = useMutation({
    mutationFn: async (values: {
      key: string;
      display_name: string;
      service_name: string;
      value_type: string;
      default_value: string;
      reload_mode: string;
      env_alias: string;
    }) => {
      const defaultValue: ConfigValue = values.value_type === 'boolean'
        ? values.default_value.trim().toLowerCase() === 'true'
        : ['integer', 'duration_ms', 'bytes'].includes(values.value_type)
          ? Number(values.default_value)
          : values.default_value;
      return api.createCustomDefinition({
        environment,
        key: values.key,
        display_name: values.display_name,
        category: 'Developer',
        scope: 'service',
        service_name: values.service_name,
        value_type: values.value_type,
        default_value: defaultValue,
        reload_mode: values.reload_mode,
        env_aliases: values.env_alias.trim() ? [values.env_alias.trim()] : [],
      });
    },
    onSuccess: async () => {
      message.success('开发参数已加入草稿');
      setCustomOpen(false);
      customForm.resetFields();
      await queryClient.invalidateQueries();
    },
    onError: (error: Error) => message.error(error.message),
  });

  const definitions = catalog.data || [];
  const areas = useMemo(() => {
    const next = new Map<string, ConfigDefinition[]>();
    definitions.forEach((definition) => {
      const key = configAreaKey(definition);
      const items = next.get(key) || [];
      items.push(definition);
      next.set(key, items);
    });
    return [...next.entries()]
      .map(([key, items]) => ({
        key,
        label: configAreaLabel(key),
        items: [...items].sort((left, right) => left.ui_order - right.ui_order),
      }))
      .sort((left, right) => {
        const order = (CONFIG_AREA_META[left.key]?.order ?? 500)
          - (CONFIG_AREA_META[right.key]?.order ?? 500);
        return order || left.label.localeCompare(right.label, 'zh-CN');
      });
  }, [definitions]);
  const selectedArea = areas.find((area) => area.key === activeArea) || areas[0];
  const normalizedSearch = configSearch.trim().toLocaleLowerCase('zh-CN');
  const visibleDefinitions = (selectedArea?.items || []).filter((definition) => {
    if (!normalizedSearch) return true;
    return [
      definition.display_name,
      definition.description,
      definition.key,
      definition.category,
      definition.service_name || '',
    ].some((value) => value.toLocaleLowerCase('zh-CN').includes(normalizedSearch));
  });
  const groups = useMemo(() => {
    const next = new Map<string, ConfigDefinition[]>();
    visibleDefinitions.forEach((definition) => {
      const items = next.get(definition.category) || [];
      items.push(definition);
      next.set(definition.category, items);
    });
    return [...next.entries()];
  }, [visibleDefinitions]);

  if (catalog.isLoading || effective.isLoading || draft.isLoading) {
    return <div className="centered"><Spin size="large" /></div>;
  }

  const currentValue = (definition: ConfigDefinition): ConfigValue =>
    Object.prototype.hasOwnProperty.call(changes, definition.key)
      ? changes[definition.key]
      : effective.data?.values[definition.key] ?? definition.default_value;

  const update = (definition: ConfigDefinition, value: ConfigValue) => {
    const baseline = effective.data?.values[definition.key] ?? definition.default_value;
    setChanges((previous) => {
      const next = { ...previous };
      if (JSON.stringify(value) === JSON.stringify(baseline)) {
        delete next[definition.key];
      } else {
        next[definition.key] = value;
      }
      return next;
    });
  };

  const selectArea = (key: string) => {
    localStorage.setItem('chatos.configuration-center.config-area', key);
    setActiveArea(key);
    setConfigSearch('');
  };

  const confirmPublish = () => {
    modal.confirm({
      title: '发布配置',
      width: 560,
      content: (
        <Space direction="vertical" style={{ width: '100%', marginTop: 12 }}>
          <Typography.Text>将发布 {Object.keys(changes).length} 项变更到 {environment}。</Typography.Text>
          <Input.TextArea
            placeholder="发布说明"
            value={publishMessage}
            onChange={(event) => setPublishMessage(event.target.value)}
          />
        </Space>
      ),
      okText: '校验并发布',
      onOk: async () => {
        await publish.mutateAsync();
      },
    });
  };

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Card>
        <Space wrap>
          <Tag color="blue">Revision {effective.data?.revision || 0}</Tag>
          <Typography.Text>草稿变更 {Object.keys(changes).length} 项</Typography.Text>
          <Button icon={<SaveOutlined />} onClick={() => save.mutate()} loading={save.isPending}>
            保存草稿
          </Button>
          <Button icon={<PlusOutlined />} onClick={() => setCustomOpen(true)}>
            新增开发参数
          </Button>
          <Button
            type="primary"
            disabled={Object.keys(changes).length === 0}
            onClick={confirmPublish}
            loading={publish.isPending}
          >
            发布
          </Button>
        </Space>
      </Card>
      <Card className="config-navigation-card">
        <div className="config-navigation-header">
          <div>
            <Typography.Title level={5} style={{ margin: 0 }}>配置分类</Typography.Title>
            <Typography.Text type="secondary">
              按服务域查看配置，当前共 {definitions.length} 项
            </Typography.Text>
          </div>
          <Input.Search
            allowClear
            value={configSearch}
            onChange={(event) => setConfigSearch(event.target.value)}
            placeholder={`搜索${selectedArea?.label || ''}配置`}
            className="config-search"
          />
        </div>
        <Tabs
          activeKey={selectedArea?.key}
          onChange={selectArea}
          className="config-area-tabs"
          items={areas.map((area) => ({
            key: area.key,
            label: (
              <span className="config-tab-label">
                {area.label}
                <span className="config-tab-count">{area.items.length}</span>
              </span>
            ),
          }))}
        />
      </Card>
      {groups.map(([category, items]) => (
        <Card
          key={category}
          title={(
            <Space size={8}>
              <span>{configCategoryLabel(category)}</span>
              <Tag>{items.length} 项</Tag>
            </Space>
          )}
        >
          <Row gutter={[24, 20]}>
            {items.map((definition) => (
              <Col xs={24} xl={12} key={definition.key}>
                <div className="config-field">
                  <Space size={6} wrap>
                    <Typography.Text strong>{definition.display_name}</Typography.Text>
                    <Tag>{definition.reload_mode}</Tag>
                    {Object.prototype.hasOwnProperty.call(changes, definition.key) && <Tag color="gold">已修改</Tag>}
                  </Space>
                  <Typography.Paragraph type="secondary" className="field-description">
                    {definition.description}
                  </Typography.Paragraph>
                  <ConfigInput
                    definition={definition}
                    value={currentValue(definition)}
                    onChange={(value) => update(definition, value)}
                  />
                  <Typography.Text type="secondary" className="field-key">
                    {definition.key}
                  </Typography.Text>
                </div>
              </Col>
            ))}
          </Row>
        </Card>
      ))}
      {groups.length === 0 && (
        <Card>
          <Empty description={configSearch ? '当前分类没有匹配的配置' : '当前分类暂无配置'} />
        </Card>
      )}
      <Modal
        open={customOpen}
        title="新增开发参数"
        okText="加入草稿"
        confirmLoading={createCustom.isPending}
        onCancel={() => setCustomOpen(false)}
        onOk={() => customForm.submit()}
      >
        <Form
          form={customForm}
          layout="vertical"
          initialValues={{
            service_name: 'chatos-backend',
            value_type: 'string',
            reload_mode: 'next_request',
          }}
          onFinish={(values) => createCustom.mutate(values)}
        >
          <Form.Item name="key" label="Key" rules={[{ required: true }]}>
            <Input placeholder="developer.feature.example" />
          </Form.Item>
          <Form.Item name="display_name" label="名称" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="service_name" label="目标服务" rules={[{ required: true }]}>
            <Select
              options={[
                'chatos-backend',
                'task-runner',
                'user-service',
                'project-service',
                'plugin-management-service',
                'local-connector-service',
                'memory-engine',
                'official-website',
              ].map((value) => ({ value, label: value }))}
            />
          </Form.Item>
          <Form.Item name="value_type" label="类型" rules={[{ required: true }]}>
            <Select
              options={['string', 'integer', 'boolean', 'duration_ms', 'bytes', 'json']
                .map((value) => ({ value, label: value }))}
            />
          </Form.Item>
          <Form.Item name="default_value" label="默认值" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="reload_mode" label="生效方式" rules={[{ required: true }]}>
            <Select
              options={['hot_reload', 'next_request', 'next_run', 'restart_required']
                .map((value) => ({ value, label: value }))}
            />
          </Form.Item>
          <Form.Item name="env_alias" label="环境变量映射">
            <Input placeholder="DEVELOPER_FEATURE_EXAMPLE" />
          </Form.Item>
        </Form>
      </Modal>
    </Space>
  );
}

function ConfigInput({
  definition,
  value,
  onChange,
}: {
  definition: ConfigDefinition;
  value: ConfigValue;
  onChange: (value: ConfigValue) => void;
}) {
  if (definition.value_type === 'boolean') {
    return <Switch checked={value === true} onChange={onChange} />;
  }
  if (definition.value_type === 'enum') {
    return (
      <Select
        value={typeof value === 'string' ? value : undefined}
        onChange={onChange}
        style={{ width: '100%' }}
        options={definition.enum_options.map((item) => ({ value: item, label: item }))}
      />
    );
  }
  if (['integer', 'duration_ms', 'bytes'].includes(definition.value_type)) {
    return (
      <InputNumber
        value={typeof value === 'number' ? value : null}
        min={definition.min ?? undefined}
        max={definition.max ?? undefined}
        onChange={(next) => onChange(next)}
        style={{ width: '100%' }}
        addonAfter={definition.value_type === 'duration_ms' ? 'ms' : definition.value_type === 'bytes' ? 'bytes' : undefined}
      />
    );
  }
  return (
    <Input
      value={typeof value === 'string' ? value : ''}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

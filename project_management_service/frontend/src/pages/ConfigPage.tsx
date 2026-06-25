import { useState } from 'react';
import type { CSSProperties } from 'react';
import { ApiOutlined, ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { Alert, Button, Descriptions, Segmented, Space, Spin, Tag, Typography } from 'antd';

import { api } from '../api/client';
import { MarkdownPreview } from '../components/MarkdownPreview';
import type { ProjectManagementSkillLocale } from '../types';

const skillLocaleOptions: Array<{ label: string; value: ProjectManagementSkillLocale }> = [
  { label: '中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' },
];

export function ConfigPage() {
  const [locale, setLocale] = useState<ProjectManagementSkillLocale>('zh-CN');
  const skillQuery = useQuery({
    queryKey: ['project-management-skill', locale],
    queryFn: () => api.getProjectManagementSkill(locale),
  });

  const skillEndpoint = `/api/skills/project-management?lang=${locale}`;
  const skill = skillQuery.data;

  return (
    <div className="page">
      <div className="page-header">
        <Space direction="vertical" size={4}>
          <Space size={10}>
            <SettingOutlined style={{ color: '#1677ff' }} />
            <Typography.Title level={3} style={{ margin: 0 }}>
              配置
            </Typography.Title>
          </Space>
          <Typography.Text type="secondary">
            查看项目管理服务对外提供给 AI agent 的 Skill 内容。
          </Typography.Text>
        </Space>
        <Space size={10} wrap>
          <Segmented<ProjectManagementSkillLocale>
            value={locale}
            options={skillLocaleOptions}
            onChange={setLocale}
          />
          <Button icon={<ReloadOutlined />} onClick={() => skillQuery.refetch()} loading={skillQuery.isFetching}>
            刷新
          </Button>
        </Space>
      </div>

      <section style={configPanelStyle}>
        <div style={panelHeaderStyle}>
          <Space size={8}>
            <ApiOutlined style={{ color: '#1677ff' }} />
            <Typography.Title level={4} style={{ margin: 0 }}>
              对外 Skill
            </Typography.Title>
          </Space>
          <Tag color="blue">Project Management MCP</Tag>
        </div>
        <Descriptions bordered column={{ xs: 1, md: 2 }} size="small">
          <Descriptions.Item label="名称">
            <Typography.Text copyable>{skill?.name || '-'}</Typography.Text>
          </Descriptions.Item>
          <Descriptions.Item label="语言">{skill?.locale || locale}</Descriptions.Item>
          <Descriptions.Item label="接口">
            <Typography.Text copyable>{skillEndpoint}</Typography.Text>
          </Descriptions.Item>
          <Descriptions.Item label="内容长度">{skill?.content.length ?? 0} 字符</Descriptions.Item>
        </Descriptions>
      </section>

      <section style={skillPreviewPanelStyle}>
        <div style={panelHeaderStyle}>
          <Typography.Title level={4} style={{ margin: 0 }}>
            Skill 内容
          </Typography.Title>
          <Tag color="blue">Markdown</Tag>
        </div>
        {skillQuery.isError ? (
          <Alert
            type="error"
            showIcon
            message="Skill 加载失败"
            description={skillQuery.error instanceof Error ? skillQuery.error.message : '请稍后重试'}
          />
        ) : (
          <Spin spinning={skillQuery.isLoading}>
            <MarkdownPreview value={skill?.content} emptyText="暂无 Skill 内容" />
          </Spin>
        )}
      </section>
    </div>
  );
}

const configPanelStyle: CSSProperties = {
  maxWidth: 1280,
  marginBottom: 16,
  background: '#fff',
  border: '1px solid #eceff3',
  borderRadius: 8,
  overflow: 'hidden',
};

const skillPreviewPanelStyle: CSSProperties = {
  maxWidth: 1280,
  background: '#fff',
  border: '1px solid #eceff3',
  borderRadius: 8,
  overflow: 'hidden',
};

const panelHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '14px 18px',
  borderBottom: '1px solid #eef0f3',
};

import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Collapse,
  Drawer,
  Empty,
  Input,
  List,
  Modal,
  Popconfirm,
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
import type {
  AiModelConfig,
  MemoryAgent,
  MemorySkill,
  MemorySkillPlugin,
  Message,
  Session,
} from '../types';

const { Text } = Typography;

interface AgentsPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
}

interface AgentEditorState {
  id?: string;
  name: string;
  description: string;
  category: string;
  roleDefinition: string;
  pluginSources: string[];
  skillIds: string[];
  enabled: boolean;
}

const EMPTY_EDITOR: AgentEditorState = {
  name: '',
  description: '',
  category: '',
  roleDefinition: '',
  pluginSources: [],
  skillIds: [],
  enabled: true,
};

export function AgentsPage({ filterUserId, currentUserId, isAdmin }: AgentsPageProps) {
  const { t } = useI18n();
  const [items, setItems] = useState<MemoryAgent[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editor, setEditor] = useState<AgentEditorState>(EMPTY_EDITOR);
  const [editorInlineSkillNames, setEditorInlineSkillNames] = useState<Record<string, string>>({});
  const [aiOpen, setAiOpen] = useState(false);
  const [aiRequirement, setAiRequirement] = useState('');
  const [aiName, setAiName] = useState('');
  const [aiCategory, setAiCategory] = useState('');
  const [aiEnabled, setAiEnabled] = useState(true);
  const [aiModelConfigs, setAiModelConfigs] = useState<AiModelConfig[]>([]);
  const [aiModelsLoading, setAiModelsLoading] = useState(false);
  const [aiModelConfigId, setAiModelConfigId] = useState('');
  const [pluginCatalog, setPluginCatalog] = useState<Record<string, MemorySkillPlugin>>({});
  const [skillCatalog, setSkillCatalog] = useState<Record<string, MemorySkill>>({});
  const [pluginPreviewOpen, setPluginPreviewOpen] = useState(false);
  const [pluginPreviewLoading, setPluginPreviewLoading] = useState(false);
  const [pluginPreviewSource, setPluginPreviewSource] = useState('');
  const [pluginPreview, setPluginPreview] = useState<MemorySkillPlugin | null>(null);
  const [pluginPreviewSkills, setPluginPreviewSkills] = useState<MemorySkill[]>([]);
  const [skillPreviewOpen, setSkillPreviewOpen] = useState(false);
  const [skillPreviewLoading, setSkillPreviewLoading] = useState(false);
  const [skillPreview, setSkillPreview] = useState<MemorySkill | null>(null);
  const [conversationOpen, setConversationOpen] = useState(false);
  const [conversationAgent, setConversationAgent] = useState<MemoryAgent | null>(null);
  const [conversationLoading, setConversationLoading] = useState(false);
  const [conversationSessions, setConversationSessions] = useState<Session[]>([]);
  const [conversationSessionId, setConversationSessionId] = useState<string | null>(null);
  const [conversationMessages, setConversationMessages] = useState<Message[]>([]);
  const [conversationMessagesLoading, setConversationMessagesLoading] = useState(false);
  const [conversationProjectNames, setConversationProjectNames] = useState<Record<string, string>>({});

  const normalizeProjectId = (projectId?: string | null): string => {
    const rawProjectId = typeof projectId === 'string'
      ? projectId.trim()
      : '';
    return rawProjectId || '0';
  };

  const groupedConversationSessions = useMemo(() => {
    const latestByProject = new Map<string, Session>();
    for (const session of conversationSessions) {
      const projectId = normalizeProjectId(session.project_id);
      const existing = latestByProject.get(projectId);
      if (!existing) {
        latestByProject.set(projectId, session);
        continue;
      }
      const existingTs = new Date(existing.updated_at).getTime();
      const currentTs = new Date(session.updated_at).getTime();
      if (Number.isNaN(existingTs) || currentTs > existingTs) {
        latestByProject.set(projectId, session);
      }
    }
    return Array.from(latestByProject.entries())
      .map(([projectId, session]) => ({
        projectId,
        projectName: (session.project_name || '').trim()
          || (conversationProjectNames[projectId] || '').trim()
          || (projectId === '0' ? t('memory.unassignedProject') : t('memory.unnamedProject')),
        session,
      }))
      .sort((left, right) => {
        const leftTs = new Date(left.session.updated_at || 0).getTime();
        const rightTs = new Date(right.session.updated_at || 0).getTime();
        return rightTs - leftTs;
      });
  }, [conversationProjectNames, conversationSessions, t]);

  const scopeUserId = useMemo(() => {
    if (!isAdmin) {
      return currentUserId.trim();
    }
    const filtered = filterUserId?.trim();
    return filtered && filtered.length > 0 ? filtered : currentUserId.trim();
  }, [isAdmin, filterUserId, currentUserId]);

  const currentUserIdTrimmed = currentUserId.trim();
  const crossScopeReadonly = isAdmin && scopeUserId !== currentUserIdTrimmed;

  const isReadonlyForScope = (agent: MemoryAgent): boolean => (
    agent.user_id !== currentUserIdTrimmed
  );

  const normalizeStringArray = (items: string[]): string[] => Array.from(
    new Set(items.map((item) => item.trim()).filter(Boolean)),
  );

  const derivePluginSourcesForSkillIds = (skillIds: string[]): string[] => normalizeStringArray(
    skillIds
      .map((skillId) => skillCatalog[skillId]?.plugin_source || '')
      .filter(Boolean),
  );

  const mergePluginSourcesWithSkills = (pluginSources: string[], skillIds: string[]): string[] => (
    normalizeStringArray([...pluginSources, ...derivePluginSourcesForSkillIds(skillIds)])
  );

  const resolvePluginDisplayName = (pluginSource: string): string => {
    const normalized = pluginSource.trim();
    if (!normalized) {
      return '-';
    }
    const plugin = pluginCatalog[normalized];
    return plugin?.name?.trim() || normalized;
  };

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const [agents, plugins, skills] = await Promise.all([
        api.listAgents(scopeUserId, { include_shared: false, limit: 200, offset: 0 }),
        api.listSkillPlugins(scopeUserId, { limit: 1000, offset: 0 }),
        api.listSkills(scopeUserId, { limit: 1000, offset: 0 }),
      ]);
      setItems(agents);
      setPluginCatalog(
        plugins.reduce<Record<string, MemorySkillPlugin>>((acc, plugin) => {
          const source = plugin.source?.trim();
          if (!source) {
            return acc;
          }
          acc[source] = plugin;
          return acc;
        }, {}),
      );
      setSkillCatalog(
        skills.reduce<Record<string, MemorySkill>>((acc, skill) => {
          acc[skill.id] = skill;
          return acc;
        }, {}),
      );
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeUserId]);

  useEffect(() => {
    if (!aiOpen) {
      return undefined;
    }

    let cancelled = false;
    const loadAiModels = async () => {
      setAiModelsLoading(true);
      try {
        const rows = await api.listModelConfigs(scopeUserId);
        if (cancelled) {
          return;
        }
        const enabledRows = rows.filter((item) => item.enabled === 1);
        setAiModelConfigs(enabledRows);
        setAiModelConfigId((prev) => {
          if (prev && enabledRows.some((item) => item.id === prev)) {
            return prev;
          }
          if (enabledRows.length === 1) {
            return enabledRows[0].id;
          }
          return '';
        });
      } catch (err) {
        if (cancelled) {
          return;
        }
        setAiModelConfigs([]);
        setAiModelConfigId('');
        setError((err as Error).message);
      } finally {
        if (!cancelled) {
          setAiModelsLoading(false);
        }
      }
    };

    loadAiModels();
    return () => {
      cancelled = true;
    };
  }, [aiOpen, scopeUserId]);

  const openCreate = () => {
    setEditor(EMPTY_EDITOR);
    setEditorInlineSkillNames({});
    setEditorOpen(true);
  };

  const openAiCreate = () => {
    setAiOpen(true);
  };

  const openEdit = (agent: MemoryAgent) => {
    const inlineNames = (agent.skills || []).reduce<Record<string, string>>((acc, skill) => {
      const skillId = skill.id?.trim();
      if (!skillId) {
        return acc;
      }
      acc[skillId] = skill.name?.trim() || t('agents.unnamedSkill');
      return acc;
    }, {});
    setEditor({
      id: agent.id,
      name: agent.name || '',
      description: agent.description || '',
      category: agent.category || '',
      roleDefinition: agent.role_definition || '',
      pluginSources: mergePluginSourcesWithSkills(
        Array.isArray(agent.plugin_sources) ? agent.plugin_sources : [],
        Array.isArray(agent.skill_ids) ? agent.skill_ids : [],
      ),
      skillIds: Array.from(new Set((agent.skill_ids || []).map((item) => item.trim()).filter(Boolean))),
      enabled: agent.enabled !== false,
    });
    setEditorInlineSkillNames(inlineNames);
    setEditorOpen(true);
  };

  const saveEditor = async () => {
    const name = editor.name.trim();
    const roleDefinition = editor.roleDefinition.trim();
    if (!name || !roleDefinition) {
      setError(t('agents.required'));
      return;
    }

    const skillIds = normalizeStringArray(editor.skillIds);
    const pluginSources = mergePluginSourcesWithSkills(editor.pluginSources, skillIds);
    setSaving(true);
    setError(null);
    try {
      if (editor.id) {
        await api.updateAgent(editor.id, {
          name,
          description: editor.description.trim() || undefined,
          category: editor.category.trim() || undefined,
          role_definition: roleDefinition,
          plugin_sources: pluginSources,
          skill_ids: skillIds,
          default_skill_ids: skillIds,
          enabled: editor.enabled,
        });
      } else {
        await api.createAgent({
          user_id: scopeUserId,
          name,
          description: editor.description.trim() || undefined,
          category: editor.category.trim() || undefined,
          role_definition: roleDefinition,
          plugin_sources: pluginSources,
          skill_ids: skillIds,
          default_skill_ids: skillIds,
          enabled: editor.enabled,
        });
      }
      setEditorOpen(false);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const removeAgent = async (agentId: string) => {
    setSaving(true);
    setError(null);
    try {
      await api.deleteAgent(agentId);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const aiModelOptions = useMemo(
    () => aiModelConfigs.map((item) => ({
      label: [item.name, item.provider, item.model].filter((part) => part && `${part}`.trim()).join(' | '),
      value: item.id,
    })),
    [aiModelConfigs],
  );

  const editorPluginOptions = useMemo(
    () => Object.values(pluginCatalog)
      .map((plugin) => ({
        value: plugin.source,
        label: [plugin.name, plugin.source, plugin.category].filter((part) => part && `${part}`.trim()).join(' | '),
      }))
      .sort((left, right) => left.label.localeCompare(right.label)),
    [pluginCatalog],
  );

  const editorSkillOptions = useMemo(() => {
    const options = new Map<string, { value: string; label: string }>();
    const selectedPluginSources = new Set(editor.pluginSources);

    Object.values(skillCatalog).forEach((skill) => {
      const skillId = skill.id?.trim();
      if (!skillId) {
        return;
      }
      const pluginSource = skill.plugin_source?.trim() || '';
      if (!selectedPluginSources.has(pluginSource) && !editor.skillIds.includes(skillId)) {
        return;
      }
      options.set(skillId, {
        value: skillId,
        label: [skill.name?.trim() || t('agents.unnamedSkill'), resolvePluginDisplayName(pluginSource)]
          .filter(Boolean)
          .join(' | '),
      });
    });

    Object.entries(editorInlineSkillNames).forEach(([skillId, skillName]) => {
      const normalizedSkillId = skillId.trim();
      if (!normalizedSkillId || options.has(normalizedSkillId)) {
        return;
      }
      const displayName = skillName.trim() || t('agents.unnamedSkill');
      options.set(normalizedSkillId, {
        value: normalizedSkillId,
        label: `${displayName} (${t('agents.inlineSkillSuffix')})`,
      });
    });

    return Array.from(options.values()).sort((left, right) => left.label.localeCompare(right.label));
  }, [editor.pluginSources, editor.skillIds, editorInlineSkillNames, pluginCatalog, skillCatalog, t]);

  const resolveSkillDisplayName = (agent: MemoryAgent, skillId: string): string => {
    const normalizedSkillId = skillId.trim();
    if (!normalizedSkillId) {
      return t('agents.unnamedSkill');
    }

    const embedded = (agent.skills || []).find((item) => item.id === normalizedSkillId);
    if (embedded?.name?.trim()) {
      return embedded.name.trim();
    }

    const catalogItem = skillCatalog[normalizedSkillId];
    if (catalogItem?.name?.trim()) {
      return catalogItem.name.trim();
    }

    return t('agents.unnamedSkill');
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

  const openPluginPreview = async (pluginSource: string) => {
    const normalizedPluginSource = pluginSource.trim();
    if (!normalizedPluginSource) {
      return;
    }

    setPluginPreviewOpen(true);
    setPluginPreviewLoading(true);
    setPluginPreviewSource(normalizedPluginSource);
    setPluginPreview(pluginCatalog[normalizedPluginSource] ?? null);
    setPluginPreviewSkills([]);
    try {
      const [pluginDetail, skills] = await Promise.all([
        api.getSkillPlugin(normalizedPluginSource, scopeUserId).catch(() => null),
        loadAllPluginSkills(normalizedPluginSource),
      ]);
      setPluginPreview(pluginDetail || pluginCatalog[normalizedPluginSource] || null);
      setPluginPreviewSkills(skills);
    } catch (err) {
      setPluginPreviewOpen(false);
      setError((err as Error).message);
    } finally {
      setPluginPreviewLoading(false);
    }
  };

  const openSkillPreview = async (agent: MemoryAgent, skillId: string) => {
    const normalizedSkillId = skillId.trim();
    if (!normalizedSkillId) {
      return;
    }

    const embedded = (agent.skills || []).find((item) => item.id === normalizedSkillId);
    setSkillPreviewOpen(true);
    setSkillPreview(null);
    setSkillPreviewLoading(true);
    try {
      try {
        const item = await api.getSkill(normalizedSkillId, scopeUserId);
        if (item) {
          setSkillPreview(item);
          return;
        }
      } catch (err) {
        if (!embedded) {
          throw err;
        }
      }

      if (embedded) {
        setSkillPreview({
          id: embedded.id,
          user_id: agent.user_id,
          plugin_source: 'inline',
          name: embedded.name || embedded.id,
          description: `Inline skill from agent ${agent.name || agent.id}`,
          content: embedded.content || '',
          source_path: '',
          version: null,
          updated_at: agent.updated_at,
        });
        return;
      }

      throw new Error(t('agents.skillNotFound'));
    } catch (err) {
      setSkillPreviewOpen(false);
      setError((err as Error).message);
    } finally {
      setSkillPreviewLoading(false);
    }
  };

  const runAiCreate = async () => {
    const requirement = aiRequirement.trim();
    if (!requirement) {
      setError(t('agents.aiRequired'));
      return;
    }

    const selectedModelConfigId = aiModelConfigId
      || (aiModelConfigs.length === 1 ? aiModelConfigs[0].id : '');
    if (aiModelConfigs.length > 1 && !selectedModelConfigId) {
      setError(t('agents.aiModelRequired'));
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await api.aiCreateAgent({
        user_id: scopeUserId,
        model_config_id: selectedModelConfigId || undefined,
        requirement,
        name: aiName.trim() || undefined,
        category: aiCategory.trim() || undefined,
        enabled: aiEnabled,
      });
      setAiRequirement('');
      setAiName('');
      setAiCategory('');
      setAiEnabled(true);
      setAiOpen(false);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const loadConversationMessages = async (sessionId: string) => {
    const normalizedSessionId = sessionId.trim();
    if (!normalizedSessionId) {
      setConversationMessages([]);
      setConversationSessionId(null);
      return;
    }
    setConversationMessagesLoading(true);
    try {
      const rows = await api.listMessages(normalizedSessionId);
      setConversationMessages(rows);
      setConversationSessionId(normalizedSessionId);
    } catch (err) {
      setError((err as Error).message);
      setConversationMessages([]);
      setConversationSessionId(normalizedSessionId);
    } finally {
      setConversationMessagesLoading(false);
    }
  };

  const openConversationDrawer = async (agent: MemoryAgent) => {
    setConversationOpen(true);
    setConversationAgent(agent);
    setConversationLoading(true);
    setConversationMessages([]);
    setConversationSessionId(null);
    setConversationProjectNames({});
    try {
      const [rows, projects] = await Promise.all([
        api.listAgentSessions(agent.id, scopeUserId, {
          limit: 120,
          offset: 0,
        }),
        api.listProjects(scopeUserId, {
          status: 'active',
          include_virtual: true,
          limit: 500,
          offset: 0,
        }),
      ]);
      const nextProjectNames: Record<string, string> = {};
      for (const project of projects) {
        const projectId = normalizeProjectId(project.project_id);
        const projectName = project.name?.trim();
        if (projectName) {
          nextProjectNames[projectId] = projectName;
        }
      }
      if (!nextProjectNames['0']) {
        nextProjectNames['0'] = t('memory.unassignedProject');
      }
      setConversationProjectNames(nextProjectNames);
      setConversationSessions(rows);
      const firstSession = rows[0];
      if (firstSession?.id) {
        await loadConversationMessages(firstSession.id);
      }
    } catch (err) {
      setError((err as Error).message);
      setConversationSessions([]);
      setConversationMessages([]);
      setConversationSessionId(null);
    } finally {
      setConversationLoading(false);
    }
  };

  const columns: ColumnsType<MemoryAgent> = [
    {
      title: t('agents.name'),
      dataIndex: 'name',
      key: 'name',
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{value || record.id.slice(0, 8)}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {record.id.slice(0, 8)}
          </Text>
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0, height: 20 }}
            onClick={(event) => {
              event.stopPropagation();
              void openConversationDrawer(record);
            }}
          >
            {t('agents.viewChats')}
          </Button>
        </Space>
      ),
    },
    {
      title: t('agents.category'),
      dataIndex: 'category',
      key: 'category',
      width: 140,
      render: (value?: string | null) => <Tag>{value || '-'}</Tag>,
    },
    {
      title: t('agents.plugins'),
      dataIndex: 'plugin_sources',
      key: 'plugin_sources',
      width: 220,
      render: (value: string[] | undefined) => {
        if (!value || value.length === 0) {
          return '-';
        }
        return (
          <Space size={[4, 4]} wrap>
            {value.map((pluginSource) => (
              <Button
                key={pluginSource}
                type="link"
                size="small"
                style={{ paddingInline: 0, height: 20 }}
                onClick={(event) => {
                  event.stopPropagation();
                  void openPluginPreview(pluginSource);
                }}
              >
                {resolvePluginDisplayName(pluginSource)}
              </Button>
            ))}
          </Space>
        );
      },
    },
    {
      title: t('agents.skills'),
      dataIndex: 'skill_ids',
      key: 'skill_ids',
      width: 220,
      render: (value: string[] | undefined, record) => {
        if (!value || value.length === 0) {
          return '-';
        }
        return (
          <Space size={[4, 4]} wrap>
            {value.map((skillId) => (
              <Button
                key={skillId}
                type="link"
                size="small"
                style={{ paddingInline: 0, height: 20 }}
                onClick={(event) => {
                  event.stopPropagation();
                  void openSkillPreview(record, skillId);
                }}
              >
                {resolveSkillDisplayName(record, skillId)}
              </Button>
            ))}
          </Space>
        );
      },
    },
    {
      title: t('agents.status'),
      dataIndex: 'enabled',
      key: 'enabled',
      width: 120,
      render: (value: boolean) => (
        <Tag color={value ? 'green' : 'default'}>{value ? t('common.enabled') : t('common.disabled')}</Tag>
      ),
    },
    {
      title: t('agents.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 220,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    {
      title: t('common.action'),
      key: 'action',
      width: 180,
      render: (_, record) => {
        const readonly = isReadonlyForScope(record);
        return (
          <Space>
            <Button
              size="small"
              onClick={(event) => {
                event.stopPropagation();
                void openConversationDrawer(record);
              }}
            >
              {t('agents.viewChats')}
            </Button>
            {readonly && <Tag color="blue">{t('agents.sharedTag')}</Tag>}
            <Button
              size="small"
              onClick={(event) => {
                event.stopPropagation();
                openEdit(record);
              }}
              disabled={readonly}
            >
              {t('common.edit')}
            </Button>
            <Popconfirm
              title={t('agents.deleteConfirm')}
              onConfirm={() => removeAgent(record.id)}
              okButtonProps={{ loading: saving }}
              disabled={readonly}
            >
              <Button
                size="small"
                danger
                disabled={readonly}
                onClick={(event) => event.stopPropagation()}
              >
                {t('common.delete')}
              </Button>
            </Popconfirm>
          </Space>
        );
      },
    },
  ];

  return (
    <Card
      title={t('agents.title')}
      extra={
        <Space>
          <Button onClick={load} loading={loading}>
            {t('common.refresh')}
          </Button>
          <Button onClick={openAiCreate} disabled={crossScopeReadonly}>
            {t('agents.aiCreate')}
          </Button>
          <Button type="primary" onClick={openCreate} disabled={crossScopeReadonly}>
            {t('agents.create')}
          </Button>
        </Space>
      }
    >
        {isAdmin && !filterUserId?.trim() && (
          <Alert type="info" showIcon message={t('agents.adminTip')} style={{ marginBottom: 12 }} />
        )}
        {!isAdmin && (
          <Alert
            type="info"
            showIcon
            message={t('agents.sharedReadonlyTip')}
            style={{ marginBottom: 12 }}
          />
        )}
        <Alert
          type="info"
          showIcon
        message={`${t('agents.scopeUser')}: ${scopeUserId || '-'}`}
        style={{ marginBottom: 12 }}
      />
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      <Table
        rowKey="id"
        loading={loading}
        dataSource={items}
        columns={columns}
        pagination={{ pageSize: 20, showSizeChanger: false }}
        onRow={(record) => ({
          onClick: () => {
            void openConversationDrawer(record);
          },
        })}
      />

      <Drawer
        open={conversationOpen}
        onClose={() => {
          setConversationOpen(false);
          setConversationAgent(null);
          setConversationSessions([]);
          setConversationProjectNames({});
          setConversationSessionId(null);
          setConversationMessages([]);
        }}
        width={980}
        styles={{ body: { paddingTop: 12, overflow: 'hidden' } }}
        title={`${t('agents.conversationsTitle')}: ${conversationAgent?.name || '-'}`}
      >
        {conversationLoading ? (
          <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 48 }}>
            <Spin />
          </div>
        ) : conversationSessions.length === 0 ? (
          <Empty description={t('agents.noConversations')} />
        ) : (
          <div
            style={{
              display: 'flex',
              gap: 12,
              width: '100%',
              height: 'calc(100vh - 210px)',
              minHeight: 420,
              overflow: 'hidden',
            }}
          >
            <Card
              title={t('agents.projectSessions')}
              style={{ width: 320, height: '100%', flexShrink: 0 }}
              styles={{ body: { height: '100%', overflowY: 'auto' } }}
            >
              <Space direction="vertical" size={10} style={{ width: '100%' }}>
                {groupedConversationSessions.map((group) => (
                  <div key={group.projectId}>
                    <Text strong style={{ color: '#0958d9', fontSize: 13 }}>
                      {group.projectName}
                    </Text>
                    <List
                      size="small"
                      dataSource={[group.session]}
                      renderItem={(session) => {
                        const active = conversationSessionId === session.id;
                        return (
                          <List.Item
                            style={{
                              cursor: 'pointer',
                              background: active ? '#f0f5ff' : undefined,
                              borderRadius: 6,
                              paddingInline: 8,
                            }}
                            onClick={() => {
                              void loadConversationMessages(session.id);
                            }}
                          >
                            <Space direction="vertical" size={0} style={{ width: '100%' }}>
                              <Text strong>{session.title || t('agents.untitledSession')}</Text>
                              <Text type="secondary" style={{ fontSize: 12 }}>
                                {new Date(session.updated_at).toLocaleString()}
                              </Text>
                            </Space>
                          </List.Item>
                        );
                      }}
                    />
                  </div>
                ))}
              </Space>
            </Card>

            <Card
              title={t('agents.messages')}
              style={{ flex: 1, minWidth: 0, height: '100%' }}
              styles={{ body: { height: '100%', overflowY: 'auto' } }}
            >
              {conversationMessagesLoading ? (
                <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 24 }}>
                  <Spin />
                </div>
              ) : conversationMessages.length === 0 ? (
                <Empty description={t('agents.noConversations')} />
              ) : (
                <List
                  size="small"
                  dataSource={conversationMessages}
                  renderItem={(message) => (
                    <List.Item>
                      <Space direction="vertical" size={2} style={{ width: '100%' }}>
                        <Space size={8}>
                          <Tag>{message.role}</Tag>
                          <Text type="secondary" style={{ fontSize: 12 }}>
                            {new Date(message.created_at).toLocaleString()}
                          </Text>
                        </Space>
                        <Text style={{ whiteSpace: 'pre-wrap' }}>
                          {message.content || '-'}
                        </Text>
                      </Space>
                    </List.Item>
                  )}
                />
              )}
            </Card>
          </div>
        )}
      </Drawer>

      <Modal
        open={editorOpen}
        title={editor.id ? t('agents.edit') : t('agents.create')}
        onCancel={() => {
          setEditorOpen(false);
          setEditorInlineSkillNames({});
        }}
        onOk={saveEditor}
        confirmLoading={saving}
        width={760}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Input
            value={editor.name}
            onChange={(event) => setEditor((prev) => ({ ...prev, name: event.target.value }))}
            placeholder={t('agents.name')}
          />
          <Input
            value={editor.category}
            onChange={(event) => setEditor((prev) => ({ ...prev, category: event.target.value }))}
            placeholder={t('agents.category')}
          />
          <Input.TextArea
            value={editor.description}
            onChange={(event) => setEditor((prev) => ({ ...prev, description: event.target.value }))}
            placeholder={t('agents.description')}
            rows={3}
          />
          <Input.TextArea
            value={editor.roleDefinition}
            onChange={(event) =>
              setEditor((prev) => ({ ...prev, roleDefinition: event.target.value }))
            }
            placeholder={t('agents.roleDefinition')}
            rows={6}
          />
          <Select
            mode="multiple"
            showSearch
            allowClear
            value={editor.pluginSources}
            onChange={(value) => setEditor((prev) => ({
              ...prev,
              pluginSources: mergePluginSourcesWithSkills(value, prev.skillIds),
            }))}
            options={editorPluginOptions}
            placeholder={t('agents.pluginSelectPlaceholder')}
            optionFilterProp="label"
            style={{ width: '100%' }}
          />
          <Select
            mode="multiple"
            showSearch
            allowClear
            value={editor.skillIds}
            onChange={(value) => setEditor((prev) => ({
              ...prev,
              skillIds: value,
              pluginSources: mergePluginSourcesWithSkills(prev.pluginSources, value),
            }))}
            options={editorSkillOptions}
            placeholder={editor.pluginSources.length > 0
              ? t('agents.skillSelectPlaceholder')
              : t('agents.skillSelectPluginFirst')}
            optionFilterProp="label"
            style={{ width: '100%' }}
            disabled={editor.pluginSources.length === 0 && editor.skillIds.length === 0}
          />
          <Space>
            <Text>{t('agents.status')}</Text>
            <Switch
              checked={editor.enabled}
              onChange={(checked) => setEditor((prev) => ({ ...prev, enabled: checked }))}
            />
          </Space>
        </Space>
      </Modal>

      <Modal
        open={aiOpen}
        title={t('agents.aiCreate')}
        onCancel={() => setAiOpen(false)}
        onOk={runAiCreate}
        confirmLoading={saving}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Input.TextArea
            value={aiRequirement}
            onChange={(event) => setAiRequirement(event.target.value)}
            placeholder={t('agents.aiRequirement')}
            rows={5}
          />
          <Select
            showSearch
            allowClear={aiModelConfigs.length !== 1}
            loading={aiModelsLoading}
            value={aiModelConfigId || undefined}
            onChange={(value) => setAiModelConfigId(value ?? '')}
            options={aiModelOptions}
            placeholder={t('agents.aiModelPlaceholder')}
            optionFilterProp="label"
          />
          <Input
            value={aiName}
            onChange={(event) => setAiName(event.target.value)}
            placeholder={t('agents.nameOptional')}
          />
          <Input
            value={aiCategory}
            onChange={(event) => setAiCategory(event.target.value)}
            placeholder={t('agents.categoryOptional')}
          />
          <Space>
            <Text>{t('agents.status')}</Text>
            <Switch checked={aiEnabled} onChange={setAiEnabled} />
          </Space>
        </Space>
      </Modal>

      <Modal
        open={pluginPreviewOpen}
        title={`${t('agents.pluginPreview')}: ${
          pluginPreview?.name?.trim()
            || resolvePluginDisplayName(pluginPreviewSource)
            || '-'
        }`}
        footer={null}
        onCancel={() => {
          setPluginPreviewOpen(false);
          setPluginPreviewSource('');
          setPluginPreview(null);
          setPluginPreviewSkills([]);
        }}
        width={920}
      >
        {pluginPreviewLoading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
            <Spin />
          </div>
        ) : (
          <Space direction="vertical" size={10} style={{ width: '100%' }}>
            <Text strong>{pluginPreviewSource || '-'}</Text>
            <Text type="secondary">
              {t('agents.pluginCategory')}: {pluginPreview?.category?.trim() || '-'}
            </Text>
            <Text type="secondary">
              {t('agents.pluginVersion')}: {pluginPreview?.version?.trim() || '-'}
            </Text>
            <Text type="secondary">
              {t('agents.pluginRepository')}: {pluginPreview?.repository?.trim() || '-'}
            </Text>
            <Text type="secondary">
              {t('agents.pluginBranch')}: {pluginPreview?.branch?.trim() || '-'}
            </Text>
            <Text strong>{t('agents.pluginDescription')}</Text>
            <div
              style={{
                maxHeight: 220,
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
                {pluginPreview?.description?.trim() || t('agents.pluginDescriptionEmpty')}
              </pre>
            </div>
            <Text strong>{t('agents.pluginMainContent')}</Text>
            <div
              style={{
                maxHeight: 280,
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
                {pluginPreview?.content?.trim() || t('agents.pluginMainContentEmpty')}
              </pre>
            </div>
            <Text strong>{t('agents.pluginCommands')}</Text>
            {(pluginPreview?.commands || []).length === 0 ? (
              <Empty description={t('agents.pluginNoCommands')} />
            ) : (
              <Collapse
                size="small"
                items={(pluginPreview?.commands || []).map((command, index) => ({
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
                        {command.content || t('agents.pluginCommandContentEmpty')}
                      </pre>
                    </div>
                  ),
                }))}
              />
            )}
            <Text strong>{t('agents.pluginSkills')}</Text>
            {pluginPreviewSkills.length === 0 ? (
              <Empty description={t('agents.pluginNoSkills')} />
            ) : (
              <Collapse
                size="small"
                defaultActiveKey={pluginPreviewSkills.map((skill) => skill.id)}
                items={pluginPreviewSkills.map((skill) => ({
                  key: skill.id,
                  label: `${skill.name || t('agents.unnamedSkill')} (${skill.id})`,
                  children: (
                    <Space direction="vertical" size={8} style={{ width: '100%' }}>
                      <Text type="secondary">
                        {t('agents.skillSourcePath')}: {skill.source_path || '-'}
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
                          {skill.content || t('agents.skillContentEmpty')}
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
        open={skillPreviewOpen}
        title={`${t('agents.skillPreview')}: ${skillPreview?.name || skillPreview?.id || '-'}`}
        footer={null}
        onCancel={() => {
          setSkillPreviewOpen(false);
          setSkillPreview(null);
        }}
        width={860}
      >
        {skillPreviewLoading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
            <Spin />
          </div>
        ) : !skillPreview ? (
          <Empty description={t('agents.skillNotFound')} />
        ) : (
          <Space direction="vertical" size={10} style={{ width: '100%' }}>
            <Text strong>{skillPreview.id}</Text>
            <Text type="secondary">
              {t('agents.skillSourceType')}: {
                skillPreview.plugin_source === 'inline'
                  ? t('agents.skillSourceInline')
                  : t('agents.skillSourceCenter')
              }
            </Text>
            <Text type="secondary">
              {t('agents.skillPluginSource')}: {skillPreview.plugin_source || '-'}
            </Text>
            <Text type="secondary">
              {t('agents.skillSourcePath')}: {skillPreview.source_path || '-'}
            </Text>
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
                {skillPreview.content || t('agents.skillContentEmpty')}
              </pre>
            </div>
          </Space>
        )}
      </Modal>
    </Card>
  );
}

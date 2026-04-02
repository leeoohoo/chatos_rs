import { useCallback, useEffect, useMemo, useState } from 'react';

import { api } from '../../api/client';
import type {
  AiModelConfig,
  MemoryAgent,
  MemorySkill,
  MemorySkillPlugin,
} from '../../types';
import {
  isReadonlyForScope as isReadonlyForScopeHelper,
  mergePluginSourcesWithSkills as mergePluginSourcesWithSkillsHelper,
  normalizeStringArray,
  resolvePluginDisplayName as resolvePluginDisplayNameHelper,
} from './helpers';
import {
  EMPTY_EDITOR,
  type AgentAiCreateState,
  type AgentEditorState,
  type AgentPageTranslate,
} from './types';

interface UseAgentsPageDataOptions {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
  t: AgentPageTranslate;
}

export interface AgentsPageDataResult {
  items: MemoryAgent[];
  loading: boolean;
  saving: boolean;
  error: string | null;
  scopeUserId: string;
  crossScopeReadonly: boolean;
  showAdminTip: boolean;
  showSharedReadonlyTip: boolean;
  refresh: () => Promise<void>;
  isReadonlyForScope: (agent: MemoryAgent) => boolean;
  resolvePluginDisplayName: (pluginSource: string) => string;
  resolveSkillDisplayName: (agent: MemoryAgent, skillId: string) => string;
  resolveModelDisplayName: (modelConfigId?: string | null) => string;
  mergePluginSourcesWithSkills: (pluginSources: string[], skillIds: string[]) => string[];
  openCreate: () => void;
  openAiCreate: () => void;
  openEdit: (agent: MemoryAgent) => void;
  removeAgent: (agentId: string) => Promise<void>;
  saveEditor: () => Promise<void>;
  updateEditor: (updater: (prev: AgentEditorState) => AgentEditorState) => void;
  editorOpen: boolean;
  closeEditor: () => void;
  editorState: AgentEditorState;
  editorPluginOptions: Array<{ value: string; label: string }>;
  editorSkillOptions: Array<{ value: string; label: string }>;
  aiCreateState: AgentAiCreateState;
  aiModelOptions: Array<{ value: string; label: string }>;
  closeAiCreate: () => void;
  updateAiCreate: (patch: Partial<AgentAiCreateState>) => void;
  runAiCreate: () => Promise<void>;
  pluginCatalog: Record<string, MemorySkillPlugin>;
  setError: (message: string | null) => void;
}

export function useAgentsPageData({
  filterUserId,
  currentUserId,
  isAdmin,
  t,
}: UseAgentsPageDataOptions): AgentsPageDataResult {
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

  const scopeUserId = useMemo(() => {
    if (!isAdmin) {
      return currentUserId.trim();
    }
    const filtered = filterUserId?.trim();
    return filtered && filtered.length > 0 ? filtered : currentUserId.trim();
  }, [currentUserId, filterUserId, isAdmin]);

  const currentUserIdTrimmed = currentUserId.trim();
  const crossScopeReadonly = isAdmin && scopeUserId !== currentUserIdTrimmed;
  const showAdminTip = isAdmin && !filterUserId?.trim();
  const showSharedReadonlyTip = !isAdmin;

  const isReadonlyForScope = (agent: MemoryAgent): boolean => (
    isReadonlyForScopeHelper(agent, currentUserIdTrimmed)
  );

  const mergePluginSourcesWithSkills = (pluginSources: string[], skillIds: string[]): string[] => (
    mergePluginSourcesWithSkillsHelper(pluginSources, skillIds, skillCatalog)
  );

  const resolvePluginDisplayName = (pluginSource: string): string => (
    resolvePluginDisplayNameHelper(pluginSource, pluginCatalog)
  );

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [agents, plugins, skills, models] = await Promise.all([
        api.listAgents(scopeUserId, { include_shared: false, limit: 200, offset: 0 }),
        api.listSkillPlugins(scopeUserId, { limit: 1000, offset: 0 }),
        api.listSkills(scopeUserId, { limit: 1000, offset: 0 }),
        api.listModelConfigs(scopeUserId),
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
      setAiModelConfigs(models.filter((item) => item.enabled === 1));
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  }, [scopeUserId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

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

    void loadAiModels();
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
      modelConfigId: agent.model_config_id || '',
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

  const closeEditor = () => {
    setEditorOpen(false);
    setEditorInlineSkillNames({});
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
          model_config_id: editor.modelConfigId.trim() || undefined,
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
          model_config_id: editor.modelConfigId.trim() || undefined,
          role_definition: roleDefinition,
          plugin_sources: pluginSources,
          skill_ids: skillIds,
          default_skill_ids: skillIds,
          enabled: editor.enabled,
        });
      }
      setEditorOpen(false);
      await refresh();
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
      await refresh();
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
  }, [editor.pluginSources, editor.skillIds, editorInlineSkillNames, resolvePluginDisplayName, skillCatalog, t]);

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

  const resolveModelDisplayName = (modelConfigId?: string | null): string => {
    const normalized = `${modelConfigId || ''}`.trim();
    if (!normalized) {
      return '-';
    }
    const model = aiModelConfigs.find((item) => item.id === normalized);
    if (!model) {
      return normalized;
    }
    return [model.name, model.provider, model.model]
      .filter((part) => part && `${part}`.trim())
      .join(' | ');
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
      await refresh();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const updateAiCreate = (patch: Partial<AgentAiCreateState>) => {
    if (patch.requirement !== undefined) {
      setAiRequirement(patch.requirement);
    }
    if (patch.name !== undefined) {
      setAiName(patch.name);
    }
    if (patch.category !== undefined) {
      setAiCategory(patch.category);
    }
    if (patch.enabled !== undefined) {
      setAiEnabled(patch.enabled);
    }
    if (patch.modelConfigId !== undefined) {
      setAiModelConfigId(patch.modelConfigId);
    }
  };

  const closeAiCreate = () => {
    setAiOpen(false);
  };

  const aiCreateState: AgentAiCreateState = {
    open: aiOpen,
    requirement: aiRequirement,
    name: aiName,
    category: aiCategory,
    enabled: aiEnabled,
    modelConfigs: aiModelConfigs,
    modelsLoading: aiModelsLoading,
    modelConfigId: aiModelConfigId,
  };

  return {
    items,
    loading,
    saving,
    error,
    scopeUserId,
    crossScopeReadonly,
    showAdminTip,
    showSharedReadonlyTip,
    refresh,
    isReadonlyForScope,
    resolvePluginDisplayName,
    resolveSkillDisplayName,
    resolveModelDisplayName,
    mergePluginSourcesWithSkills,
    openCreate,
    openAiCreate,
    openEdit,
    removeAgent,
    saveEditor,
    updateEditor: (updater) => {
      setEditor((prev) => updater(prev));
    },
    editorOpen,
    closeEditor,
    editorState: editor,
    editorPluginOptions,
    editorSkillOptions,
    aiCreateState,
    aiModelOptions,
    closeAiCreate,
    updateAiCreate,
    runAiCreate,
    pluginCatalog,
    setError,
  };
}

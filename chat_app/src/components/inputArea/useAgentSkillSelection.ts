import { useCallback, useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { AgentConfig } from '../../types';

type AgentRuntimeSkill = NonNullable<AgentConfig['runtime_skills']>[number];
type AgentRuntimePlugin = NonNullable<AgentConfig['runtime_plugins']>[number];
type AgentInlineSkill = NonNullable<AgentConfig['skills']>[number];

const readArrayField = <T,>(value: unknown, fallback: T[] | undefined): T[] | undefined => (
  Array.isArray(value) ? value as T[] : fallback
);

interface UseAgentSkillSelectionOptions {
  client: ApiClient;
  currentAgent?: AgentConfig | null;
}

export const useAgentSkillSelection = ({
  client,
  currentAgent,
}: UseAgentSkillSelectionOptions) => {
  const [skillsEnabled, setSkillsEnabled] = useState(false);
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>([]);
  const [resolvedAgentForSkills, setResolvedAgentForSkills] = useState<AgentConfig | null>(null);
  const [skillsLoading, setSkillsLoading] = useState(false);

  const currentAgentForSkills = useMemo<AgentConfig | null>(
    () => (currentAgent && typeof currentAgent === 'object' ? currentAgent : null),
    [currentAgent],
  );

  useEffect(() => {
    let cancelled = false;
    const baseAgent = currentAgentForSkills;
    const agentId = typeof baseAgent?.id === 'string' ? baseAgent.id.trim() : '';
    if (!baseAgent || !agentId) {
      setResolvedAgentForSkills(null);
      setSkillsLoading(false);
      return undefined;
    }
    const hasRuntimeSkills = Array.isArray(baseAgent.runtime_skills)
      && baseAgent.runtime_skills.length > 0;
    const hasInlineSkills = Array.isArray(baseAgent.skills)
      && baseAgent.skills.length > 0;
    if (hasRuntimeSkills || hasInlineSkills) {
      setResolvedAgentForSkills(baseAgent);
      setSkillsLoading(false);
      return undefined;
    }

    setResolvedAgentForSkills(baseAgent);
    setSkillsLoading(true);
    void client.getMemoryAgentRuntimeContext(agentId)
      .then((runtime) => {
        if (cancelled) {
          return;
        }
        setResolvedAgentForSkills({
          ...baseAgent,
          runtime_skills: readArrayField<AgentRuntimeSkill>(runtime?.runtime_skills, []),
          runtime_plugins: readArrayField<AgentRuntimePlugin>(runtime?.runtime_plugins, []),
          plugin_sources: readArrayField<string>(runtime?.plugin_sources, baseAgent.plugin_sources),
          skills: readArrayField<AgentInlineSkill>(runtime?.skills, baseAgent.skills),
          skill_ids: readArrayField<string>(runtime?.skill_ids, baseAgent.skill_ids),
        });
      })
      .catch(() => {
        if (!cancelled) {
          setResolvedAgentForSkills(baseAgent);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setSkillsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [client, currentAgentForSkills]);

  const availableSkillOptions = useMemo(() => {
    const byId = new Map<string, { id: string; name: string; description?: string | null }>();
    const runtimeSkills = Array.isArray(resolvedAgentForSkills?.runtime_skills)
      ? resolvedAgentForSkills.runtime_skills
      : [];
    runtimeSkills.forEach((skill) => {
      const id = typeof skill?.id === 'string' ? skill.id.trim() : '';
      if (!id || byId.has(id)) return;
      const name = typeof skill?.name === 'string' && skill.name.trim().length > 0
        ? skill.name.trim()
        : id;
      const description = typeof skill?.description === 'string' ? skill.description.trim() : '';
      byId.set(id, {
        id,
        name,
        description: description || null,
      });
    });
    const inlineSkills = Array.isArray(resolvedAgentForSkills?.skills)
      ? resolvedAgentForSkills.skills
      : [];
    inlineSkills.forEach((skill) => {
      const id = typeof skill?.id === 'string' ? skill.id.trim() : '';
      if (!id || byId.has(id)) return;
      const name = typeof skill?.name === 'string' && skill.name.trim().length > 0
        ? skill.name.trim()
        : id;
      byId.set(id, { id, name, description: null });
    });
    return Array.from(byId.values());
  }, [resolvedAgentForSkills]);

  useEffect(() => {
    setSelectedSkillIds((prev) => prev.filter((id) => availableSkillOptions.some((item) => item.id === id)));
  }, [availableSkillOptions]);

  useEffect(() => {
    if (!resolvedAgentForSkills) {
      setSkillsEnabled(false);
      setSelectedSkillIds([]);
    }
  }, [resolvedAgentForSkills]);

  const handleToggleSelectedSkill = useCallback((skillId: string) => {
    const normalized = typeof skillId === 'string' ? skillId.trim() : '';
    if (!normalized) return;
    setSelectedSkillIds((prev) => (
      prev.includes(normalized)
        ? prev.filter((item) => item !== normalized)
        : [...prev, normalized]
    ));
  }, []);

  const handleClearSelectedSkills = useCallback(() => {
    setSelectedSkillIds([]);
  }, []);

  return {
    currentAgentForSkills: resolvedAgentForSkills,
    skillsEnabled,
    setSkillsEnabled,
    skillsLoading,
    availableSkillOptions,
    selectedSkillIds,
    handleToggleSelectedSkill,
    handleClearSelectedSkills,
  };
};

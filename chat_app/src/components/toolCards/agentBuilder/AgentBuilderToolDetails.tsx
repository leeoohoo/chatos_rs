import React from 'react';

import { RowsCard, StringListCard, TextBlockCard, renderCardHeader } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

const truncateText = (value: string, maxLength: number = 260): string => {
  const trimmed = value.trim();
  if (trimmed.length <= maxLength) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxLength - 1)}...`;
};

const SkillItemsCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const skills = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (skills.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Available skills', `${skills.length} 个`)}
      <div className="tool-detail-list">
        {skills.map((skill, index) => (
          <div key={`agent-skill-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(skill.name).trim() || asString(skill.id).trim() || `skill ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              {[asString(skill.id).trim(), asString(skill.source).trim()].filter(Boolean).join(' · ')}
            </div>
            <div className="tool-detail-item-body">
              {asString(skill.content_preview ?? skill.contentPreview).trim()}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const EmbeddedSkillsCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const skills = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (skills.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Embedded skills', `${skills.length} 个`)}
      <div className="tool-detail-list">
        {skills.map((skill, index) => (
          <div key={`embedded-skill-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(skill.name).trim() || asString(skill.id).trim() || `skill ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              {asString(skill.id).trim()}
            </div>
            <div className="tool-detail-item-body">
              {truncateText(asString(skill.content).trim())}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const AgentCard: React.FC<{ title: string; value: unknown }> = ({ title, value }) => {
  const agent = asRecord(value);
  if (!agent) return null;

  const pluginSources = asArray(agent.plugin_sources ?? agent.pluginSources)
    .map((item) => asString(item))
    .filter(Boolean);
  const skillIds = asArray(agent.skill_ids ?? agent.skillIds)
    .map((item) => asString(item))
    .filter(Boolean);
  const defaultSkillIds = asArray(agent.default_skill_ids ?? agent.defaultSkillIds)
    .map((item) => asString(item))
    .filter(Boolean);
  const embeddedSkills = asArray(agent.skills);

  return (
    <>
      <RowsCard
        title={title}
        rows={[
          { key: 'name', value: asString(agent.name).trim() },
          { key: 'category', value: asString(agent.category).trim() },
          { key: 'enabled', value: asBoolean(agent.enabled) },
          { key: 'plugin sources', value: pluginSources.length },
          { key: 'embedded skills', value: embeddedSkills.length },
          { key: 'skill ids', value: skillIds.length },
          { key: 'default skill ids', value: defaultSkillIds.length },
        ]}
        fullWidth
      />
      <TextBlockCard title="Description" content={asString(agent.description)} fullWidth={false} />
      <TextBlockCard title="Role definition" content={asString(agent.role_definition ?? agent.roleDefinition)} />
      <StringListCard
        title="Plugin sources"
        values={pluginSources}
        fullWidth
      />
      <StringListCard
        title="Skill IDs"
        values={skillIds}
        fullWidth
      />
      <StringListCard
        title="Default skill IDs"
        values={defaultSkillIds}
        fullWidth
      />
      <EmbeddedSkillsCard items={embeddedSkills} />
    </>
  );
};

interface AgentBuilderToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const AgentBuilderToolDetails: React.FC<AgentBuilderToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  if (displayName === 'recommend_agent_profile') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Recommended profile"
          rows={[
            { key: 'name', value: asString(record.name).trim() },
            { key: 'category', value: asString(record.category).trim() },
          ]}
          fullWidth
        />
        <TextBlockCard title="Description" content={asString(record.description)} fullWidth={false} />
        <TextBlockCard title="Role definition" content={asString(record.role_definition ?? record.roleDefinition)} />
        <StringListCard
          title="Suggested skills"
          values={asArray(record.suggested_skill_ids ?? record.suggestedSkillIds).map((item) => asString(item)).filter(Boolean)}
          fullWidth
        />
      </div>
    );
  }

  if (displayName === 'list_available_skills') {
    return (
      <div className="tool-detail-stack">
        <RowsCard title="Skill catalog" rows={[{ key: 'count', value: asNumber(record.count) }]} />
        <SkillItemsCard items={asArray(record.items)} />
      </div>
    );
  }

  if (displayName === 'create_memory_agent' || displayName === 'update_memory_agent') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title={displayName === 'create_memory_agent' ? 'Creation result' : 'Update result'}
          rows={[
            { key: 'created', value: asBoolean(record.created) },
            { key: 'updated', value: asBoolean(record.updated) },
          ]}
        />
        <AgentCard title="Agent" value={record.agent} />
      </div>
    );
  }

  if (displayName === 'preview_agent_context') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Context preview"
          rows={[
            { key: 'role chars', value: asNumber(record.role_definition_chars ?? record.roleDefinitionChars) },
            { key: 'plugin sources', value: asNumber(record.plugin_sources_count ?? record.pluginSourcesCount) },
            { key: 'skills', value: asNumber(record.skills_count ?? record.skillsCount) },
            { key: 'skill ids', value: asNumber(record.skill_ids_count ?? record.skillIdsCount) },
          ]}
        />
        <TextBlockCard title="Preview" content={asString(record.preview)} />
      </div>
    );
  }

  return null;
};

export default AgentBuilderToolDetails;

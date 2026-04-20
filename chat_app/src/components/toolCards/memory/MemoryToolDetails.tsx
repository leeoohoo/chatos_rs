import React from 'react';

import { RowsCard, TextBlockCard, renderCardHeader } from '../shared/primitives';
import { asArray, asNumber, asRecord, asString } from '../shared/value';

const PluginCommandListCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const commands = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (commands.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Plugin commands', `${commands.length} 个`)}
      <div className="tool-detail-list">
        {commands.map((command, index) => (
          <div key={`plugin-command-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(command.name).trim() || `command ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              {[asString(command.source_path ?? command.sourcePath).trim(), asString(command.argument_hint ?? command.argumentHint).trim()].filter(Boolean).join(' · ')}
            </div>
            <div className="tool-detail-item-body">
              {asString(command.description).trim()}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const RelatedSkillsCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const skills = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (skills.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Related skills', `${skills.length} 个`)}
      <div className="tool-detail-list">
        {skills.map((skill, index) => (
          <div key={`related-skill-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(skill.name).trim() || asString(skill.id).trim() || `skill ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              {[asString(skill.source_type ?? skill.sourceType).trim(), asString(skill.source_path ?? skill.sourcePath).trim()].filter(Boolean).join(' · ')}
            </div>
            <div className="tool-detail-item-body">
              {asString(skill.description).trim()}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

interface MemoryToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const MemoryToolDetails: React.FC<MemoryToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  if (displayName === 'get_command_detail') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Command"
          rows={[
            { key: 'ref', value: asString(record.command_ref ?? record.commandRef).trim() },
            { key: 'name', value: asString(record.name).trim() },
            { key: 'plugin source', value: asString(record.plugin_source ?? record.pluginSource).trim() },
            { key: 'source path', value: asString(record.source_path ?? record.sourcePath).trim() },
            { key: 'argument hint', value: asString(record.argument_hint ?? record.argumentHint).trim() },
            { key: 'updated at', value: asString(record.updated_at ?? record.updatedAt).trim() },
          ]}
          fullWidth
        />
        <TextBlockCard title="Command description" content={asString(record.description)} fullWidth={false} />
        <TextBlockCard title="Command content" content={asString(record.content)} />
      </div>
    );
  }

  if (displayName === 'get_skill_detail') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Skill"
          rows={[
            { key: 'ref', value: asString(record.skill_ref ?? record.skillRef).trim() },
            { key: 'name', value: asString(record.name).trim() },
            { key: 'source type', value: asString(record.source_type ?? record.sourceType).trim() },
            { key: 'plugin source', value: asString(record.plugin_source ?? record.pluginSource).trim() },
            { key: 'source path', value: asString(record.source_path ?? record.sourcePath).trim() },
            { key: 'updated at', value: asString(record.updated_at ?? record.updatedAt).trim() },
          ]}
          fullWidth
        />
        <TextBlockCard title="Skill description" content={asString(record.description)} fullWidth={false} />
        <TextBlockCard title="Skill content" content={asString(record.content)} />
      </div>
    );
  }

  if (displayName === 'get_plugin_detail') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Plugin"
          rows={[
            { key: 'ref', value: asString(record.plugin_ref ?? record.pluginRef).trim() },
            { key: 'name', value: asString(record.name).trim() },
            { key: 'source', value: asString(record.source).trim() },
            { key: 'category', value: asString(record.category).trim() },
            { key: 'version', value: asString(record.version).trim() },
            { key: 'repository', value: asString(record.repository).trim() },
            { key: 'branch', value: asString(record.branch).trim() },
            { key: 'commands', value: asNumber(record.command_count ?? record.commandCount) },
            { key: 'updated at', value: asString(record.updated_at ?? record.updatedAt).trim() },
          ]}
          fullWidth
        />
        <TextBlockCard title="Plugin description" content={asString(record.description)} fullWidth={false} />
        <TextBlockCard title="Plugin content" content={asString(record.content)} />
        <PluginCommandListCard items={asArray(record.commands)} />
        <RelatedSkillsCard items={asArray(record.related_skills ?? record.relatedSkills)} />
      </div>
    );
  }

  return null;
};

export default MemoryToolDetails;

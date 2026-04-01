import React from 'react';

import { cn } from '../../../lib/utils';
import type { Project } from '../../../types';

interface InputAreaProjectSelectorProps {
  showProjectSelector: boolean;
  availableProjects: Project[];
  selectedProjectId?: string | null;
  onProjectChange?: (projectId: string | null) => void;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
}

export const InputAreaProjectSelector: React.FC<InputAreaProjectSelectorProps> = ({
  showProjectSelector,
  availableProjects,
  selectedProjectId,
  onProjectChange,
  disabled,
  isStreaming,
  isStopping,
}) => {
  if (!showProjectSelector || availableProjects.length === 0) {
    return null;
  }

  return (
    <select
      value={selectedProjectId || ''}
      onChange={(event) => onProjectChange?.(event.target.value || null)}
      disabled={disabled || isStreaming || isStopping}
      className={cn(
        'flex-shrink-0 px-2 py-1 text-xs rounded-md border bg-background',
        'text-foreground focus:outline-none focus:ring-1 focus:ring-primary',
        (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
      )}
      title="发送时透传 project_root"
    >
      <option value="">请选择项目</option>
      {availableProjects.map((project) => (
        <option key={project.id} value={project.id}>
          {project.name}
        </option>
      ))}
    </select>
  );
};

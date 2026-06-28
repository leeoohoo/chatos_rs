import type { ProjectWorkItemRecord, RequirementStatus, RequirementType } from '../../types';

export const requirementStatusDisplayOptions = [
  { value: 'draft', label: '鑽夌' },
  { value: 'reviewing', label: '璇勫涓?' },
  { value: 'approved', label: '宸茬‘璁?' },
  { value: 'in_progress', label: '瀹炵幇涓?' },
  { value: 'done', label: '宸插畬鎴?' },
  { value: 'cancelled', label: '宸插彇娑?' },
  { value: 'archived', label: '宸插綊妗?' },
] satisfies Array<{ value: RequirementStatus; label: string }>;

export const requirementStatusOptions = requirementStatusDisplayOptions.filter(
  (option) => option.value !== 'archived',
);

export const requirementTypeOptions = [
  { value: 'requirement', label: '闇€姹?' },
  { value: 'change', label: '鍙樻洿' },
  { value: 'bug_fix', label: 'Bug 淇' },
] satisfies Array<{ value: RequirementType; label: string }>;

export const workItemStatusDisplayOptions = [
  { value: 'todo', label: '寰呭鐞?' },
  { value: 'ready', label: '宸插氨缁?' },
  { value: 'in_progress', label: '杩涜涓?' },
  { value: 'blocked', label: '闃诲' },
  { value: 'done', label: '瀹屾垚' },
  { value: 'cancelled', label: '鍙栨秷' },
  { value: 'archived', label: '宸插綊妗?' },
] satisfies Array<{ value: ProjectWorkItemRecord['status']; label: string }>;

export const workItemStatusOptions = workItemStatusDisplayOptions.filter(
  (option) => option.value !== 'archived',
);

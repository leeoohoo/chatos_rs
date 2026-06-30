import type {
  CreateWorkItemPayload,
  DependencyGraphEdge,
  DependencyGraphNode,
  ProjectWorkItemRecord,
  RequirementRecord,
} from '../../types';

export type WorkItemFormValues = CreateWorkItemPayload & {
  requirement_id: string;
  tags_text?: string;
};

export interface GraphRelationRow {
  key: string;
  edge: DependencyGraphEdge;
  from?: DependencyGraphNode;
  to?: DependencyGraphNode;
}

export type RequirementTableRecord = RequirementRecord & {
  children?: RequirementTableRecord[];
  tree_level?: number;
};

export type ProfileMarkdownFieldName = 'background' | 'introduction';
export type ExecutionOptionLabelMap = Map<string, string>;

export const emptyRequirements: RequirementRecord[] = [];
export const emptyWorkItems: ProjectWorkItemRecord[] = [];

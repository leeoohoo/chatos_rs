import { z } from "zod";

export const projectTypeDefinitionSchema = z.object({
  key: z.string().min(1),
  label: z.string().min(1),
  description: z.string(),
  categoryKey: z.string().min(1),
  categoryLabel: z.string().min(1),
});

export const projectDirectorySelectionSchema = z.object({
  selectionId: z.string().min(1),
  directoryName: z.string().min(1),
});

export const createdLocalProjectSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  revision: z.number().int().positive(),
  projectTypeKey: z.string().min(1),
});

export type ProjectTypeDefinition = z.infer<typeof projectTypeDefinitionSchema>;
export type ProjectDirectorySelection = z.infer<typeof projectDirectorySelectionSchema>;
export type CreatedLocalProject = z.infer<typeof createdLocalProjectSchema>;

export interface CreateLocalProjectInput {
  ownerUserId: string;
  presentationId: string;
  selectionId: string;
  name: string;
  description: string;
  projectTypeKey: string;
}

export function normalizedProjectName(value: string): string {
  return value.trim();
}

export function canSubmitProject(input: {
  projectTypes: ProjectTypeDefinition[];
  selection: ProjectDirectorySelection | null;
  projectName: string;
  projectTypeKey: string;
  isLoadingTypes: boolean;
  isPicking: boolean;
  isSaving: boolean;
}): boolean {
  return normalizedProjectName(input.projectName).length > 0
    && input.selection != null
    && input.projectTypes.some((projectType) => projectType.key === input.projectTypeKey)
    && !input.isLoadingTypes
    && !input.isPicking
    && !input.isSaving;
}

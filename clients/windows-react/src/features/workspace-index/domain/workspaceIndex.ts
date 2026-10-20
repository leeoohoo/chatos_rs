import { z } from "zod";

export const localProjectSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  revision: z.number().int().positive(),
  projectTypeKey: z.string().min(1),
});

export const workspaceContactSchema = z.object({
  id: z.string().min(1),
  agentId: z.string().min(1),
  name: z.string().min(1),
  status: z.string().nullable().optional(),
});

export const workspaceConversationSchema = z.object({
  id: z.string().min(1),
  title: z.string(),
  projectId: z.string().nullable().optional(),
  contactId: z.string().nullable().optional(),
  updatedAtUnixMs: z.number().int().nonnegative(),
});

export const workspaceRelationsSchema = z.object({
  contacts: z.array(workspaceContactSchema),
  conversations: z.array(workspaceConversationSchema),
});

export type LocalProject = z.infer<typeof localProjectSchema>;
export type WorkspaceContact = z.infer<typeof workspaceContactSchema>;
export type WorkspaceConversation = z.infer<typeof workspaceConversationSchema>;
export type WorkspaceRelations = z.infer<typeof workspaceRelationsSchema>;

export interface WorkspaceResourceItem {
  id: string;
  title: string;
  subtitle: string | null;
  conversationId: string | null;
}

export interface WorkspaceIndexSnapshot {
  contacts: WorkspaceResourceItem[];
  projects: WorkspaceResourceItem[];
}

export type WorkspaceSelection =
  | { kind: "workbench" }
  | { kind: "terminal" }
  | { kind: "contact"; id: string }
  | { kind: "project"; id: string }
  | { kind: "remote"; id: string };

export function resolveWorkspaceIndex(
  projects: LocalProject[],
  relations: WorkspaceRelations,
): WorkspaceIndexSnapshot {
  const conversations = [...relations.conversations].sort(
    (left, right) => right.updatedAtUnixMs - left.updatedAtUnixMs,
  );
  const contacts = relations.contacts
    .map((contact) => {
      const conversation = conversations.find(
        (candidate) => candidate.projectId == null && candidate.contactId === contact.id,
      );
      return {
        id: contact.id,
        title: contact.name,
        subtitle: conversation?.title ?? statusTitle(contact.status),
        conversationId: conversation?.id ?? null,
      };
    })
    .sort(resourceTitleOrder);
  const resolvedProjects = projects
    .map((project) => {
      const conversation = conversations.find(
        (candidate) => candidate.projectId === project.id,
      );
      return {
        id: project.id,
        title: project.name,
        subtitle: conversation?.title ?? null,
        conversationId: conversation?.id ?? null,
      };
    })
    .sort(resourceTitleOrder);
  return { contacts, projects: resolvedProjects };
}

function statusTitle(status?: string | null): string | null {
  switch (status?.toLowerCase()) {
    case "active":
      return "已连接";
    case "disabled":
      return "已停用";
    default:
      return null;
  }
}

function resourceTitleOrder(
  left: WorkspaceResourceItem,
  right: WorkspaceResourceItem,
): number {
  return left.title.localeCompare(right.title, undefined, { sensitivity: "base" });
}


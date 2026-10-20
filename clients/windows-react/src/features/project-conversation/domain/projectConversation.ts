import { z } from "zod";

export const preparedProjectConversationSchema = z.object({
  projectId: z.string().min(1),
  conversationId: z.string().min(1),
});

export type PreparedProjectConversation = z.infer<typeof preparedProjectConversationSchema>;

export type ProjectConversationPhase =
  | { kind: "idle" }
  | { kind: "preparing" }
  | { kind: "available"; conversationId: string }
  | { kind: "failed"; message: string };

export const idleProjectConversation: ProjectConversationPhase = { kind: "idle" };

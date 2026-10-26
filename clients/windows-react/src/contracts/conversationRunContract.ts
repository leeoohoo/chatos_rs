import { z } from "zod";
import { LOCAL_AGENT_PROTOCOL_VERSION } from "../features/host-lifecycle/domain/hostContract";

const successEnvelope = <Result extends z.ZodType>(result: Result) =>
  z.object({
    protocol_version: z.literal(LOCAL_AGENT_PROTOCOL_VERSION),
    command_id: z.string().min(1),
    ok: z.literal(true),
    result,
    error: z.null().optional(),
  });

const conversationIdentitySchema = z.object({
  conversation_id: z.string().min(1),
  owner_user_id: z.string().min(1),
  title: z.string(),
  resource: z
    .object({
      kind: z.enum(["contact", "project"]),
      resource_id: z.string().min(1),
    })
    .nullable(),
  version: z.number().int().positive(),
  created_at_unix_ms: z.number().int().nonnegative(),
  updated_at_unix_ms: z.number().int().nonnegative(),
});

const turnSchema = z.object({
  turn_id: z.string().min(1),
  conversation_id: z.string().min(1),
  user_message_id: z.string().min(1),
  run_id: z.string().min(1),
  status: z.enum(["running", "succeeded", "failed", "cancelled"]),
  created_at_unix_ms: z.number().int().nonnegative(),
  updated_at_unix_ms: z.number().int().nonnegative(),
});

const messageSchema = z.object({
  message_id: z.string().min(1),
  conversation_id: z.string().min(1),
  turn_id: z.string().min(1),
  ordinal: z.number().int().positive(),
  role: z.enum(["user", "assistant"]),
  content: z.unknown(),
  metadata: z.unknown(),
  created_at_unix_ms: z.number().int().nonnegative(),
});

const attachmentSchema = z.object({
  attachment_id: z.string().min(1),
  conversation_id: z.string().min(1),
  turn_id: z.string().min(1),
  message_id: z.string().min(1),
  ordinal: z.number().int().positive(),
  display_name: z.string().min(1),
  media_type: z.string().min(1),
  byte_size: z.number().int().nonnegative(),
  sha256: z.string().regex(/^[a-f0-9]{64}$/),
  authorized_local_ref: z.string().startsWith("local-attachment:"),
  metadata: z.unknown(),
  created_at_unix_ms: z.number().int().nonnegative(),
});

export const conversationHistoryPageSchema = z.object({
  conversation: conversationIdentitySchema,
  turns: z.array(turnSchema),
  messages: z.array(messageSchema),
  attachments: z.array(attachmentSchema),
  next_before_ordinal: z.number().int().positive().nullable(),
});

export const conversationHistoryResponseSchema = successEnvelope(
  z.object({
    type: z.literal("conversation_history"),
    page: conversationHistoryPageSchema,
  }),
);

export type ConversationHistoryPage = z.infer<typeof conversationHistoryPageSchema>;

const taskSchema = z.object({
  graph_id: z.string().min(1),
  owner_user_id: z.string().min(1),
  source_entity_type: z.string().min(1),
  source_entity_id: z.string().min(1),
  task_id: z.string().min(1),
  title: z.string().min(1),
  model_config_ref: z.string().min(1),
  model_config_revision: z.string().min(1),
  input: z.unknown(),
  status: z.enum(["pending", "ready", "running", "succeeded", "failed", "cancelled", "blocked"]),
  active_run_id: z.string().min(1).nullable(),
  version: z.number().int().positive(),
  created_at_unix_ms: z.number().int().nonnegative(),
  updated_at_unix_ms: z.number().int().nonnegative(),
});

export const taskDisplayStatusSchema = z.enum([
  "pending", "ready", "queued", "running", "succeeded", "failed", "cancelled", "blocked",
  "reviewing_followup", "waiting_confirmation",
]);

export const messageTaskGraphSchema = z.object({
      root_task_ids: z.array(z.string().min(1)),
      nodes: z.array(
        z.object({
          task: taskSchema,
          depth: z.number().int().nonnegative(),
          is_root: z.boolean(),
          is_current_message: z.boolean(),
        }),
      ),
      edges: z.array(
        z.object({
          source_task_id: z.string().min(1),
          target_task_id: z.string().min(1),
          kind: z.string().min(1),
        }),
      ),
      source_conversation_id: z.string().min(1),
      source_turn_id: z.string().min(1),
      source_user_message_id: z.string().min(1).nullable(),
    });

export const messageTaskGraphResponseSchema = successEnvelope(
  z.object({
    type: z.literal("message_task_graph"),
    graph: messageTaskGraphSchema,
  }),
);

export type MessageTaskGraph = z.infer<typeof messageTaskGraphSchema>;

export const runEventSchema = z.object({
  cursor: z.number().int().nonnegative(),
  event_id: z.string().min(1),
  run_id: z.string().min(1),
  event_type: z.string().min(1),
  payload: z.unknown(),
  created_at_unix_ms: z.number().int().nonnegative(),
});

export const runEventsResponseSchema = successEnvelope(
  z.object({
    type: z.literal("events"),
    events: z.array(runEventSchema),
    next_cursor: z.number().int().nonnegative(),
  }),
);

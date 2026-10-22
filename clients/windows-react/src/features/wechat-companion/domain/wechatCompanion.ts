import { z } from "zod";

export const weChatBindingSchema = z.object({
  bound: z.boolean(),
  createdAt: z.string().nullable(),
  lastLoginAt: z.string().nullable(),
}).strict();

export const weChatClientSessionSchema = z.object({
  id: z.string().uuid(),
  clientType: z.literal("wechat_mini_program"),
  createdAt: z.string().min(1),
  lastSeenAt: z.string().min(1),
  expiresAtUnix: z.number().int().positive(),
  revokedAt: z.string().nullable(),
}).strict();

export const weChatCompanionSnapshotSchema = z.object({
  binding: weChatBindingSchema,
  sessions: z.array(weChatClientSessionSchema).max(1_000),
}).strict();

export const weChatBindTicketSchema = z.object({
  status: z.enum(["issued", "claimed", "expired"]),
  expiresAtUnix: z.number().int().positive(),
  previewUrl: z.string().min(1),
}).strict();

export type WeChatCompanionSnapshot = z.infer<typeof weChatCompanionSnapshotSchema>;
export type WeChatClientSession = z.infer<typeof weChatClientSessionSchema>;
export type WeChatBindTicket = z.infer<typeof weChatBindTicketSchema>;

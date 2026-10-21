import { z } from "zod";

export const agentProfessionKeySchema = z.enum([
  "project_manager",
  "product_manager",
  "technical_manager",
  "solution_architect",
  "security_engineer",
  "qa_engineer",
  "software_engineer",
  "fullstack_engineer",
  "frontend_engineer",
  "backend_engineer",
  "mobile_engineer",
  "desktop_engineer",
  "game_engineer",
  "embedded_iot_engineer",
  "database_engineer",
  "devops_engineer",
  "data_engineer",
  "data_analyst",
  "machine_learning_engineer",
  "research_specialist",
  "product_designer",
  "ui_designer",
  "ux_designer",
  "game_designer",
  "technical_writer",
  "business_analyst",
  "implementation_consultant",
  "erp_consultant",
  "wms_consultant",
  "domain_expert",
  "operations_specialist",
  "growth_marketing_specialist",
  "general_member",
]);

export const agentThinkingLevelSchema = z.enum([
  "auto",
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
]);

export const agentProfileSummarySchema = z.object({
  id: z.string().trim().min(1).max(512),
  name: z.string().trim().min(1).max(120),
  description: z.string().trim().max(2_000),
  modelConfigId: z.string().trim().min(1).max(512),
  thinkingLevel: agentThinkingLevelSchema.nullable(),
  professionKey: agentProfessionKeySchema,
  heartbeatEnabled: z.boolean(),
  heartbeatIntervalSeconds: z.number().int().min(60).max(86_400),
  lastHeartbeatAtUnixMs: z.number().int().nonnegative().nullable(),
  nextHeartbeatAtUnixMs: z.number().int().nonnegative().nullable(),
  canManageStaff: z.boolean(),
  canAccessLocalProjects: z.boolean(),
  hasAvatar: z.boolean(),
  avatarPreviewUrl: z.string().max(2_048).refine((value) => {
    try {
      const url = new URL(value);
      const native = url.protocol === "chatos-agent:" && url.hostname === "localhost";
      const windows = url.protocol === "http:" && url.hostname === "chatos-agent.localhost" && url.port === "";
      const parts = url.pathname.split("/").filter(Boolean);
      return (native || windows)
        && url.username === ""
        && url.password === ""
        && url.search === ""
        && url.hash === ""
        && parts.length === 3
        && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u.test(parts[0] ?? "")
        && /^[A-Za-z0-9._-]{1,512}$/u.test(parts[1] ?? "")
        && /^(?:0|[1-9][0-9]*)$/u.test(parts[2] ?? "");
    } catch {
      return false;
    }
  }).nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((profile, context) => {
  if (profile.updatedAtUnixMs < profile.createdAtUnixMs) {
    context.addIssue({
      code: "custom",
      message: "Agent 更新时间不能早于创建时间。",
      path: ["updatedAtUnixMs"],
    });
  }
  if (profile.hasAvatar !== (profile.avatarPreviewUrl != null)) {
    context.addIssue({
      code: "custom",
      message: "Agent 头像状态与安全预览不一致。",
      path: ["avatarPreviewUrl"],
    });
  }
});

export const agentProfileListSchema = z.array(agentProfileSummarySchema).max(10_000)
  .superRefine((profiles, context) => {
    const identifiers = new Set<string>();
    profiles.forEach((profile, index) => {
      if (identifiers.has(profile.id)) {
        context.addIssue({
          code: "custom",
          message: "Agent 标识不能重复。",
          path: [index, "id"],
        });
      }
      identifiers.add(profile.id);
    });
  });

export type AgentProfileSummary = z.infer<typeof agentProfileSummarySchema>;

export const agentModelOptionSchema = z.object({
  id: z.string().trim().min(1).max(512),
  displayName: z.string().trim().min(1).max(500),
  modelName: z.string().trim().min(1).max(500),
  provider: z.string().trim().min(1).max(200),
  defaultThinkingLevel: agentThinkingLevelSchema.nullable(),
  supportsReasoning: z.boolean(),
  thinkingLevels: z.array(agentThinkingLevelSchema).max(8),
}).strict();

export const agentProfessionOptionSchema = z.object({
  key: agentProfessionKeySchema,
  label: z.string().trim().min(1).max(200),
  description: z.string().trim().min(1).max(2_000),
  categoryKey: z.string().trim().min(1).max(200),
  categoryLabel: z.string().trim().min(1).max(200),
}).strict();

export const agentProfileEditorCatalogSchema = z.object({
  models: z.array(agentModelOptionSchema).max(1_000),
  professions: z.array(agentProfessionOptionSchema).length(33),
  defaultRolePrompt: z.string().trim().min(1).max(32_000),
}).strict();

export const agentProfileEditorDataSchema = z.object({
  profile: agentProfileSummarySchema,
  rolePrompt: z.string().trim().min(1).max(32_000),
  heartbeatPrompt: z.string().trim().max(8_000),
}).strict();

export const agentAvatarDraftSchema = z.object({
  id: z.string().regex(/^reference_[0-9a-f]{32}$/u),
  name: z.string().trim().min(1).max(1_024),
  mimeType: z.enum(["image/png", "image/jpeg", "image/webp"]),
  byteSize: z.number().int().positive().max(20 * 1_024 * 1_024),
  previewUrl: z.string().max(2_048).refine((value) => {
    try {
      const url = new URL(value);
      const native = url.protocol === "chatos-media:" && url.hostname === "localhost";
      const windows = url.protocol === "http:" && url.hostname === "chatos-media.localhost" && url.port === "";
      return (native || windows)
        && url.username === ""
        && url.password === ""
        && url.search === ""
        && url.hash === "";
    } catch {
      return false;
    }
  }).nullable(),
}).strict();

export type AgentAvatarAction =
  | { kind: "preserve" }
  | { kind: "remove" }
  | { kind: "staged"; presentationId: string; referenceId: string }
  | { kind: "history"; recordId: string; contentToken: string };

export interface AgentAvatarHistoryAsset {
  recordId: string;
  contentToken: string;
  previewUrl: string;
  createdAtUnixMs: number;
}

export interface AgentProfileMutationInput {
  agentId: string | null;
  expectedUpdatedAtUnixMs: number | null;
  name: string;
  description: string;
  rolePrompt: string;
  modelConfigId: string;
  thinkingLevel: z.infer<typeof agentThinkingLevelSchema> | null;
  professionKey: z.infer<typeof agentProfessionKeySchema>;
  canManageStaff: boolean;
  canAccessLocalProjects: boolean;
  heartbeatEnabled: boolean;
  heartbeatIntervalSeconds: number;
  heartbeatPrompt: string;
  avatar: AgentAvatarAction;
}

export type AgentProfileEditorCatalog = z.infer<typeof agentProfileEditorCatalogSchema>;
export type AgentProfileEditorData = z.infer<typeof agentProfileEditorDataSchema>;
export type AgentAvatarDraft = z.infer<typeof agentAvatarDraftSchema>;

export const preparedAgentConversationSchema = z.object({
  roomId: z.string().uuid(),
  agentId: z.string().trim().min(1).max(512),
  conversationId: z.string().regex(/^agent-direct-[0-9a-f-]{36}$/u),
  title: z.string().trim().min(1).max(120),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((conversation, context) => {
  if (conversation.updatedAtUnixMs < conversation.createdAtUnixMs) {
    context.addIssue({
      code: "custom",
      message: "Agent 私聊更新时间不能早于创建时间。",
      path: ["updatedAtUnixMs"],
    });
  }
  if (conversation.conversationId !== `agent-direct-${conversation.roomId}`) {
    context.addIssue({
      code: "custom",
      message: "Agent 私聊身份不一致。",
      path: ["conversationId"],
    });
  }
});

export type PreparedAgentConversation = z.infer<typeof preparedAgentConversationSchema>;

export const agentConversationActivityItemSchema = z.object({
  deliveryId: z.string().trim().min(1).max(512),
  roomId: z.string().uuid(),
  agentId: z.string().trim().min(1).max(512),
  turnId: z.string().trim().min(1).max(512),
  runId: z.string().trim().min(1).max(512),
  deliveryStatus: z.enum(["pending", "running", "completed", "failed", "cancelled"]),
  runStatus: z.enum(["ready", "running", "succeeded", "failed", "cancelled"]),
  responseMessageId: z.string().trim().min(1).max(512).nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((item, context) => {
  if (item.updatedAtUnixMs < item.createdAtUnixMs) {
    context.addIssue({
      code: "custom",
      message: "Agent Run 更新时间不能早于创建时间。",
      path: ["updatedAtUnixMs"],
    });
  }
  const expectedRunStatus = {
    pending: "ready",
    running: "running",
    completed: "succeeded",
    failed: "failed",
    cancelled: "cancelled",
  }[item.deliveryStatus];
  if (item.runStatus !== expectedRunStatus) {
    context.addIssue({
      code: "custom",
      message: "Agent Delivery 与 Run 状态不一致。",
      path: ["runStatus"],
    });
  }
});

export const agentConversationActivitySnapshotSchema = z.object({
  roomId: z.string().uuid(),
  agentId: z.string().trim().min(1).max(512),
  conversationId: z.string().regex(/^agent-direct-[0-9a-f-]{36}$/u),
  conversationVersion: z.number().int().positive(),
  hasActiveDelivery: z.boolean(),
  items: z.array(agentConversationActivityItemSchema).max(100),
}).strict().superRefine((snapshot, context) => {
  if (snapshot.conversationId !== `agent-direct-${snapshot.roomId}`) {
    context.addIssue({
      code: "custom",
      message: "Agent 运行动态会话身份不一致。",
      path: ["conversationId"],
    });
  }
  const identifiers = new Set<string>();
  snapshot.items.forEach((item, index) => {
    if (item.roomId !== snapshot.roomId || item.agentId !== snapshot.agentId) {
      context.addIssue({
        code: "custom",
        message: "Agent 运行动态成员身份不一致。",
        path: ["items", index],
      });
    }
    for (const identity of [item.deliveryId, item.turnId, item.runId]) {
      if (identifiers.has(identity)) {
        context.addIssue({
          code: "custom",
          message: "Agent 运行动态标识不能重复。",
          path: ["items", index],
        });
      }
      identifiers.add(identity);
    }
  });
  const hasActiveDelivery = snapshot.items.some(({ deliveryStatus }) => (
    deliveryStatus === "pending" || deliveryStatus === "running"
  ));
  if (snapshot.hasActiveDelivery !== hasActiveDelivery) {
    context.addIssue({
      code: "custom",
      message: "Agent 活跃 Delivery 状态不一致。",
      path: ["hasActiveDelivery"],
    });
  }
});

export type AgentConversationActivitySnapshot = z.infer<typeof agentConversationActivitySnapshotSchema>;

export const agentTeamMemberActivitySnapshotSchema = z.object({
  roomId: z.string().uuid(),
  agentId: z.string().trim().min(1).max(512),
  conversationId: z.string().trim().min(1).max(1_100),
  conversationVersion: z.number().int().nonnegative(),
  hasActiveDelivery: z.boolean(),
  items: z.array(agentConversationActivityItemSchema).max(100),
}).strict().superRefine((snapshot, context) => {
  if (snapshot.conversationId !== `agent-team-${snapshot.roomId}-${snapshot.agentId}`) {
    context.addIssue({
      code: "custom",
      message: "团队 Agent 运行动态会话身份不一致。",
      path: ["conversationId"],
    });
  }
  const identifiers = new Set<string>();
  snapshot.items.forEach((item, index) => {
    if (item.roomId !== snapshot.roomId || item.agentId !== snapshot.agentId) {
      context.addIssue({
        code: "custom",
        message: "团队 Agent 运行动态成员身份不一致。",
        path: ["items", index],
      });
    }
    for (const identity of [item.deliveryId, item.turnId, item.runId]) {
      if (identifiers.has(identity)) {
        context.addIssue({
          code: "custom",
          message: "团队 Agent 运行动态标识不能重复。",
          path: ["items", index],
        });
      }
      identifiers.add(identity);
    }
  });
  const hasActiveDelivery = snapshot.items.some(({ deliveryStatus }) => (
    deliveryStatus === "pending" || deliveryStatus === "running"
  ));
  if (snapshot.hasActiveDelivery !== hasActiveDelivery) {
    context.addIssue({
      code: "custom",
      message: "团队 Agent 活跃 Delivery 状态不一致。",
      path: ["hasActiveDelivery"],
    });
  }
});

export type AgentTeamMemberActivitySnapshot = z.infer<typeof agentTeamMemberActivitySnapshotSchema>;

export const agentProfileActivitySnapshotSchema = z.object({
  agentId: z.string().trim().min(1).max(512),
  hasActiveDelivery: z.boolean(),
  items: z.array(agentConversationActivityItemSchema).max(100),
}).strict().superRefine((snapshot, context) => {
  const identifiers = new Set<string>();
  snapshot.items.forEach((item, index) => {
    if (item.agentId !== snapshot.agentId) {
      context.addIssue({
        code: "custom",
        message: "Agent 运行动态成员身份不一致。",
        path: ["items", index, "agentId"],
      });
    }
    for (const identity of [item.deliveryId, item.turnId, item.runId]) {
      if (identifiers.has(identity)) {
        context.addIssue({
          code: "custom",
          message: "Agent 运行动态标识不能重复。",
          path: ["items", index],
        });
      }
      identifiers.add(identity);
    }
  });
  const hasActiveDelivery = snapshot.items.some(({ deliveryStatus }) => (
    deliveryStatus === "pending" || deliveryStatus === "running"
  ));
  if (snapshot.hasActiveDelivery !== hasActiveDelivery) {
    context.addIssue({
      code: "custom",
      message: "Agent 活跃 Delivery 状态不一致。",
      path: ["hasActiveDelivery"],
    });
  }
});

export type AgentProfileActivitySnapshot = z.infer<typeof agentProfileActivitySnapshotSchema>;

export const agentTeamSummarySchema = z.object({
  id: z.string().uuid(),
  projectId: z.string().trim().min(1).max(512),
  name: z.string().trim().min(1).max(160),
  goal: z.string().trim().max(8_000),
  defaultAgentId: z.string().trim().min(1).max(512).nullable(),
  projectManagerAgentId: z.string().trim().min(1).max(512).nullable(),
  memberCount: z.number().int().min(0).max(10_000),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((team, context) => {
  if (team.updatedAtUnixMs < team.createdAtUnixMs) {
    context.addIssue({
      code: "custom",
      message: "Agent 团队更新时间不能早于创建时间。",
      path: ["updatedAtUnixMs"],
    });
  }
});

export const agentTeamListSchema = z.array(agentTeamSummarySchema).max(1_000)
  .superRefine((teams, context) => {
    const teamIds = new Set<string>();
    const projectIds = new Set<string>();
    teams.forEach((team, index) => {
      if (teamIds.has(team.id) || projectIds.has(team.projectId)) {
        context.addIssue({
          code: "custom",
          message: "同一项目只能有一个活动 Agent 团队。",
          path: [index],
        });
      }
      teamIds.add(team.id);
      projectIds.add(team.projectId);
    });
  });

export const agentTeamMemberSummarySchema = z.object({
  agentId: z.string().trim().min(1).max(512),
  role: z.string().trim().min(1).max(160),
  responsibility: z.string().trim().max(8_000),
  joinedAtUnixMs: z.number().int().nonnegative(),
}).strict();

export const agentTeamDetailSchema = z.object({
  team: agentTeamSummarySchema,
  members: z.array(agentTeamMemberSummarySchema).max(1_000),
}).strict().superRefine((detail, context) => {
  const agentIds = new Set<string>();
  detail.members.forEach((member, index) => {
    if (agentIds.has(member.agentId)) {
      context.addIssue({
        code: "custom",
        message: "Agent 团队成员不能重复。",
        path: ["members", index, "agentId"],
      });
    }
    agentIds.add(member.agentId);
  });
  if (detail.team.memberCount !== detail.members.length) {
    context.addIssue({
      code: "custom",
      message: "Agent 团队成员数量不一致。",
      path: ["team", "memberCount"],
    });
  }
  for (const [field, agentId] of [
    ["defaultAgentId", detail.team.defaultAgentId],
    ["projectManagerAgentId", detail.team.projectManagerAgentId],
  ] as const) {
    if (agentId != null && !agentIds.has(agentId)) {
      context.addIssue({
        code: "custom",
        message: "Agent 团队负责人必须是活动成员。",
        path: ["team", field],
      });
    }
  }
});

export const agentTeamMessageSchema = z.object({
  id: z.string().trim().min(1).max(512),
  senderKind: z.enum(["human", "agent", "system"]),
  senderId: z.string().trim().min(1).max(512),
  content: z.string().max(1_000_000),
  mentionedAgentIds: z.array(z.string().trim().min(1).max(512)).max(100),
  createdAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((message, context) => {
  if (new Set(message.mentionedAgentIds).size !== message.mentionedAgentIds.length) {
    context.addIssue({ code: "custom", message: "团队消息中的 @Agent 不能重复。", path: ["mentionedAgentIds"] });
  }
});

export const agentTeamDeliverySchema = z.object({
  id: z.string().trim().min(1).max(512),
  messageId: z.string().trim().min(1).max(512),
  targetAgentId: z.string().trim().min(1).max(512),
  triggerKind: z.enum(["mention", "default_agent", "agent_mention", "heartbeat", "todo", "todo_status"]),
  status: z.enum(["pending", "running", "completed", "failed", "cancelled"]),
  responseMessageId: z.string().trim().min(1).max(512).nullable(),
  lastError: z.string().max(2_000).nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
  completedAtUnixMs: z.number().int().nonnegative().nullable(),
}).strict();

export const agentTeamConversationSnapshotSchema = z.object({
  teamId: z.string().uuid(),
  messages: z.array(agentTeamMessageSchema).max(200),
  deliveries: z.array(agentTeamDeliverySchema).max(500),
}).strict().superRefine((snapshot, context) => {
  const messageIds = new Set<string>();
  snapshot.messages.forEach((message, index) => {
    if (messageIds.has(message.id)) {
      context.addIssue({ code: "custom", message: "团队消息不能重复。", path: ["messages", index, "id"] });
    }
    messageIds.add(message.id);
  });
  const deliveryIds = new Set<string>();
  snapshot.deliveries.forEach((delivery, index) => {
    if (deliveryIds.has(delivery.id)) {
      context.addIssue({ code: "custom", message: "团队 Delivery 不能重复。", path: ["deliveries", index, "id"] });
    }
    deliveryIds.add(delivery.id);
  });
});

export const queuedAgentTeamMessageSchema = z.object({
  messageId: z.string().trim().min(1).max(512),
  teamId: z.string().uuid(),
  createdAtUnixMs: z.number().int().nonnegative(),
  deliveries: z.array(z.object({
    deliveryId: z.string().trim().min(1).max(512),
    targetAgentId: z.string().trim().min(1).max(512),
    triggerKind: z.enum(["mention", "default_agent"]),
  }).strict()).max(100),
}).strict();

export const dispatchedAgentTeamDeliverySchema = z.object({
  deliveryId: z.string().trim().min(1).max(512),
  teamId: z.string().uuid(),
  agentId: z.string().trim().min(1).max(512),
  turnId: z.string().trim().min(1).max(512),
  runId: z.string().trim().min(1).max(512),
}).strict();

export const agentTeamRunSummarySchema = z.object({
  deliveryId: z.string().trim().min(1).max(512),
  messageId: z.string().trim().min(1).max(512),
  agentId: z.string().trim().min(1).max(512),
  lane: z.enum(["manager", "executor"]),
  triggerKind: z.enum(["mention", "default_agent", "agent_mention", "heartbeat", "todo", "todo_status"]),
  deliveryStatus: z.enum(["pending", "running", "completed", "failed", "cancelled"]),
  runStatus: z.enum(["ready", "running", "succeeded", "failed", "cancelled"]),
  turnId: z.string().trim().min(1).max(512),
  runId: z.string().trim().min(1).max(512),
  responseMessageId: z.string().trim().min(1).max(512).nullable(),
  lastError: z.string().max(2_000).nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((run, context) => {
  const expectedRunStatus = {
    pending: "ready",
    running: "running",
    completed: "succeeded",
    failed: "failed",
    cancelled: "cancelled",
  }[run.deliveryStatus];
  if (run.runStatus !== expectedRunStatus) {
    context.addIssue({ code: "custom", message: "团队 Delivery 与 Run 状态不一致。", path: ["runStatus"] });
  }
  if ((run.lane === "executor") !== (run.triggerKind === "todo")) {
    context.addIssue({ code: "custom", message: "团队 Run 执行通道与触发来源不一致。", path: ["lane"] });
  }
  if (run.updatedAtUnixMs < run.createdAtUnixMs) {
    context.addIssue({ code: "custom", message: "团队 Run 更新时间不能早于创建时间。", path: ["updatedAtUnixMs"] });
  }
});

export const agentTeamRunHistorySnapshotSchema = z.object({
  teamId: z.string().uuid(),
  messageCount: z.number().int().nonnegative(),
  pendingDeliveryCount: z.number().int().nonnegative(),
  runningDeliveryCount: z.number().int().nonnegative(),
  completedDeliveryCount: z.number().int().nonnegative(),
  failedDeliveryCount: z.number().int().nonnegative(),
  cancelledDeliveryCount: z.number().int().nonnegative(),
  runs: z.array(agentTeamRunSummarySchema).max(500),
}).strict().superRefine((snapshot, context) => {
  const identities = new Set<string>();
  snapshot.runs.forEach((run, index) => {
    for (const identity of [run.deliveryId, run.turnId, run.runId]) {
      if (identities.has(identity)) {
        context.addIssue({ code: "custom", message: "团队 Run 标识不能重复。", path: ["runs", index] });
      }
      identities.add(identity);
    }
  });
  const deliveryCount = snapshot.pendingDeliveryCount + snapshot.runningDeliveryCount
    + snapshot.completedDeliveryCount + snapshot.failedDeliveryCount + snapshot.cancelledDeliveryCount;
  if (deliveryCount < snapshot.runs.length) {
    context.addIssue({ code: "custom", message: "团队 Delivery 汇总少于已记录 Run。", path: ["runs"] });
  }
});

export const agentTeamAssetCategorySchema = z.enum([
  "overview", "current_progress", "tech_stack", "architecture",
  "conventions", "decision", "reference",
]);

export const agentTeamAssetSchema = z.object({
  id: z.string().trim().min(1).max(512),
  teamId: z.string().uuid(),
  category: agentTeamAssetCategorySchema,
  title: z.string().trim().min(1).max(240),
  markdown: z.string().max(128_000),
  revision: z.number().int().positive(),
  status: z.enum(["active", "archived"]),
  createdByAgentId: z.string().trim().min(1).max(512).nullable(),
  updatedByAgentId: z.string().trim().min(1).max(512).nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((asset, context) => {
  if (asset.updatedAtUnixMs < asset.createdAtUnixMs) {
    context.addIssue({ code: "custom", message: "团队资产更新时间不能早于创建时间。", path: ["updatedAtUnixMs"] });
  }
});

export const agentTeamAssetListSchema = z.array(agentTeamAssetSchema).max(1_000)
  .superRefine((assets, context) => {
    const identities = new Set<string>();
    assets.forEach((asset, index) => {
      if (asset.status !== "active") {
        context.addIssue({ code: "custom", message: "活动资产列表不能包含已归档资产。", path: [index, "status"] });
      }
      if (identities.has(asset.id)) {
        context.addIssue({ code: "custom", message: "团队共享资产不能重复。", path: [index, "id"] });
      }
      identities.add(asset.id);
    });
  });

export const agentTeamAssetRevisionSchema = z.object({
  assetId: z.string().trim().min(1).max(512),
  revision: z.number().int().positive(),
  title: z.string().trim().min(1).max(240),
  markdown: z.string().max(128_000),
  editorAgentId: z.string().trim().min(1).max(512).nullable(),
  createdAtUnixMs: z.number().int().nonnegative(),
}).strict();

export const agentTeamAssetRevisionListSchema = z.array(agentTeamAssetRevisionSchema).max(500)
  .superRefine((revisions, context) => {
    const identities = new Set<number>();
    revisions.forEach((revision, index) => {
      if (identities.has(revision.revision)) {
        context.addIssue({ code: "custom", message: "团队资产版本不能重复。", path: [index, "revision"] });
      }
      identities.add(revision.revision);
      if (index > 0 && revisions[index - 1]!.revision <= revision.revision) {
        context.addIssue({ code: "custom", message: "团队资产版本必须按新到旧排列。", path: [index, "revision"] });
      }
    });
  });

export interface CreateAgentTeamInput {
  projectId: string;
  name: string;
  goal: string;
}

export interface UpdateAgentTeamMemberInput {
  role: string;
  responsibility: string;
}

export interface SaveAgentTeamAssetInput {
  assetId: string | null;
  category: z.infer<typeof agentTeamAssetCategorySchema>;
  title: string;
  markdown: string;
  expectedRevision: number | null;
}

export type AgentTeamSummary = z.infer<typeof agentTeamSummarySchema>;
export type AgentTeamDetail = z.infer<typeof agentTeamDetailSchema>;
export type AgentTeamConversationSnapshot = z.infer<typeof agentTeamConversationSnapshotSchema>;
export type QueuedAgentTeamMessage = z.infer<typeof queuedAgentTeamMessageSchema>;
export type DispatchedAgentTeamDelivery = z.infer<typeof dispatchedAgentTeamDeliverySchema>;
export type AgentTeamRunSummary = z.infer<typeof agentTeamRunSummarySchema>;
export type AgentTeamRunHistorySnapshot = z.infer<typeof agentTeamRunHistorySnapshotSchema>;
export type AgentTeamAssetCategory = z.infer<typeof agentTeamAssetCategorySchema>;
export type AgentTeamAsset = z.infer<typeof agentTeamAssetSchema>;
export type AgentTeamAssetRevision = z.infer<typeof agentTeamAssetRevisionSchema>;

export const agentProfessionLabels: Record<z.infer<typeof agentProfessionKeySchema>, string> = {
  project_manager: "项目经理",
  product_manager: "产品经理",
  technical_manager: "技术经理",
  solution_architect: "解决方案架构师",
  security_engineer: "安全工程师",
  qa_engineer: "质量工程师",
  software_engineer: "软件工程师",
  fullstack_engineer: "全栈工程师",
  frontend_engineer: "前端工程师",
  backend_engineer: "后端工程师",
  mobile_engineer: "移动端工程师",
  desktop_engineer: "桌面端工程师",
  game_engineer: "游戏工程师",
  embedded_iot_engineer: "嵌入式与 IoT 工程师",
  database_engineer: "数据库工程师",
  devops_engineer: "DevOps 工程师",
  data_engineer: "数据工程师",
  data_analyst: "数据分析师",
  machine_learning_engineer: "机器学习工程师",
  research_specialist: "研究专员",
  product_designer: "产品设计师",
  ui_designer: "UI 设计师",
  ux_designer: "UX 设计师",
  game_designer: "游戏设计师",
  technical_writer: "技术写作者",
  business_analyst: "业务分析师",
  implementation_consultant: "实施顾问",
  erp_consultant: "ERP 顾问",
  wms_consultant: "WMS 仓储顾问",
  domain_expert: "领域专家",
  operations_specialist: "运营专员",
  growth_marketing_specialist: "增长与市场专员",
  general_member: "通用成员",
};

export function heartbeatIntervalLabel(seconds: number) {
  if (seconds === 60) return "每分钟";
  if (seconds === 300) return "每 5 分钟";
  if (seconds === 900) return "每 15 分钟";
  if (seconds === 1_800) return "每 30 分钟";
  if (seconds === 3_600) return "每小时";
  return `每 ${Math.floor(seconds / 60)} 分钟`;
}

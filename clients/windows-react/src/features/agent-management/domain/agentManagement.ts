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

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

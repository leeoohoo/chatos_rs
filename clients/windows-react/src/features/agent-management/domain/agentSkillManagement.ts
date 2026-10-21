import { z } from "zod";

export const agentSkillKindSchema = z.enum(["profession", "projectType"]);

export const agentSkillResourceSchema = z.object({
  relativePath: z.string().regex(/^references\/[A-Za-z0-9._-]+\.md$/u),
  title: z.string().trim().min(1).max(200),
  summary: z.string().trim().min(1).max(2_000),
  markdown: z.string().trim().min(1).max(100_000),
  sizeBytes: z.number().int().positive().max(1_000_000),
}).strict().superRefine((resource, context) => {
  if (new TextEncoder().encode(resource.markdown).byteLength !== resource.sizeBytes) {
    context.addIssue({ code: "custom", message: "Skill 资源大小不匹配。", path: ["sizeBytes"] });
  }
});

export const agentSkillDefinitionSchema = z.object({
  kind: agentSkillKindSchema,
  key: z.string().trim().min(1).max(200),
  label: z.string().trim().min(1).max(120),
  labelEn: z.string().trim().min(1).max(120),
  description: z.string().trim().min(1).max(4_000),
  descriptionEn: z.string().trim().min(1).max(4_000),
  categoryKey: z.string().trim().min(1).max(200),
  categoryLabel: z.string().trim().min(1).max(200),
  categoryLabelEn: z.string().trim().min(1).max(200),
  markdown: z.string().trim().min(1).max(500_000),
  markdownEn: z.string().trim().min(1).max(500_000),
  customized: z.boolean(),
  skillRef: z.string().regex(/^AS-(?:profession|project_type)-[A-Za-z0-9._-]+-[0-9a-f]{12}$/u),
  skillRefEn: z.string().regex(/^AS-(?:profession|project_type)-[A-Za-z0-9._-]+-[0-9a-f]{12}$/u),
  instructionsSha256: z.string().regex(/^[0-9a-f]{64}$/u),
  instructionsSha256En: z.string().regex(/^[0-9a-f]{64}$/u),
  resources: z.array(agentSkillResourceSchema).length(4),
  resourcesEn: z.array(agentSkillResourceSchema).length(4),
}).strict().superRefine((item, context) => {
  const kind = item.kind === "projectType" ? "project_type" : "profession";
  const expected = `AS-${kind}-${item.key}-${item.instructionsSha256.slice(0, 12)}`;
  const expectedEn = `AS-${kind}-${item.key}-${item.instructionsSha256En.slice(0, 12)}`;
  if (item.skillRef !== expected) {
    context.addIssue({ code: "custom", message: "Skill 引用与正文摘要不匹配。", path: ["skillRef"] });
  }
  if (item.skillRefEn !== expectedEn) {
    context.addIssue({ code: "custom", message: "英文 Skill 引用与正文摘要不匹配。", path: ["skillRefEn"] });
  }
  for (const field of ["resources", "resourcesEn"] as const) {
    const paths = new Set(item[field].map((resource) => resource.relativePath));
    if (paths.size !== item[field].length) {
      context.addIssue({ code: "custom", message: "Skill 资源路径不能重复。", path: [field] });
    }
  }
});

export const agentSkillCatalogSchema = z.array(agentSkillDefinitionSchema).length(60)
  .superRefine((items, context) => {
    const identities = new Set<string>();
    items.forEach((item, index) => {
      const identity = `${item.kind}:${item.key}`;
      if (identities.has(identity)) {
        context.addIssue({
          code: "custom",
          message: "Skill 标识不能重复。",
          path: [index, "key"],
        });
      }
      identities.add(identity);
    });
  });

export type AgentSkillKind = z.infer<typeof agentSkillKindSchema>;
export type AgentSkillDefinition = z.infer<typeof agentSkillDefinitionSchema>;

export interface AgentSkillMutationInput {
  kind: AgentSkillKind;
  key: string;
  label: string;
  labelEn: string;
  description: string;
  descriptionEn: string;
  markdown: string;
  markdownEn: string;
}

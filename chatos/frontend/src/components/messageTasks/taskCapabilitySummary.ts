// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { isRecord, readStringArray } from './utils';

export interface TaskCapabilitySummary {
  builtinLabels: string[];
  externalMcpIds: string[];
  skillLabels: string[];
  hasComputerUse: boolean;
  hasAnyCapability: boolean;
}

const BUILTIN_LABELS: Record<string, string> = {
  BrowserTools: '浏览器工具',
  TerminalController: '终端控制',
  CodeMaintainerRead: '代码/文件读取',
  CodeMaintainerWrite: '代码/文件写入',
  ProjectManagement: '项目管理',
  TaskManager: '任务管理',
  AskUser: '询问用户',
};

const SKILL_LABELS: Record<string, string> = {
  internal_skill_computer_use: 'Computer Use / 电脑控制',
  internal_skill_browser: '浏览器控制 Skill',
  internal_skill_chrome: 'Chrome 控制 Skill',
  internal_skill_documents: '文档处理',
  internal_skill_pdf: 'PDF 处理',
  internal_skill_presentations: '演示文稿处理',
  internal_skill_spreadsheets: '表格处理',
  internal_skill_excel_live_control: 'Excel 实时控制',
  internal_skill_imagegen: '图像生成',
  internal_skill_visualize: '可视化',
  internal_skill_openai_docs: 'OpenAI 文档',
};

const uniqueLabels = (
  values: string[],
  labels: Record<string, string>,
): string[] => {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const value of values) {
    const label = labels[value] || value;
    if (!seen.has(label)) {
      seen.add(label);
      out.push(label);
    }
  }
  return out;
};

export const taskCapabilitySummary = (mcpConfig: unknown): TaskCapabilitySummary => {
  const config = isRecord(mcpConfig) ? mcpConfig : {};
  const builtinIds = readStringArray(config.enabled_builtin_kinds);
  const skillIds = readStringArray(config.selected_skill_ids);
  const externalMcpIds = readStringArray(config.external_mcp_config_ids);
  return {
    builtinLabels: uniqueLabels(builtinIds, BUILTIN_LABELS),
    externalMcpIds,
    skillLabels: uniqueLabels(skillIds, SKILL_LABELS),
    hasComputerUse: skillIds.includes('internal_skill_computer_use'),
    hasAnyCapability: builtinIds.length > 0 || skillIds.length > 0 || externalMcpIds.length > 0,
  };
};

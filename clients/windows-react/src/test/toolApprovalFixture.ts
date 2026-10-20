import type { ToolApproval } from "../features/tool-approval/domain/toolApproval";

export function sampleToolApproval(
  overrides: Partial<ToolApproval> = {},
): ToolApproval {
  return {
    invocationId: "invocation-1",
    invocationVersion: 2,
    runId: "run-1",
    toolName: "execute_command",
    command: "cargo test",
    purpose: "验证当前改动",
    cwd: "clients/windows-react",
    source: "Local Agent Task",
    risk: "medium",
    reason: "命令可能修改文件、依赖或远程状态。",
    createdAtUnixMs: 1_700_000_000_000,
    updatedAtUnixMs: 1_700_000_000_100,
    availableDecisions: ["decline", "acceptForSession", "accept"],
    ...overrides,
  };
}

import { describe, expect, it } from "vitest";
import { agentProfileListSchema, heartbeatIntervalLabel } from "./agentManagement";

describe("agent management domain", () => {
  it("accepts the exact safe native projection", () => {
    const parsed = agentProfileListSchema.parse([fixture()]);
    expect(parsed[0]?.professionKey).toBe("general_member");
    expect(heartbeatIntervalLabel(parsed[0]!.heartbeatIntervalSeconds)).toBe("每 15 分钟");
  });

  it("rejects private profile fields and inconsistent timestamps", () => {
    expect(() => agentProfileListSchema.parse([{ ...fixture(), rolePrompt: "private" }])).toThrow();
    expect(() => agentProfileListSchema.parse([{
      ...fixture(),
      createdAtUnixMs: 20,
      updatedAtUnixMs: 10,
    }])).toThrow();
  });

  it("rejects duplicate profile identities", () => {
    expect(() => agentProfileListSchema.parse([fixture(), fixture()])).toThrow();
  });
});

function fixture() {
  return {
    id: "agent-1",
    name: "设计师",
    description: "负责界面体验",
    modelConfigId: "model-1",
    thinkingLevel: "high" as const,
    professionKey: "general_member" as const,
    heartbeatEnabled: true,
    heartbeatIntervalSeconds: 900,
    lastHeartbeatAtUnixMs: 100,
    nextHeartbeatAtUnixMs: 200,
    canManageStaff: false,
    canAccessLocalProjects: true,
    hasAvatar: false,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}

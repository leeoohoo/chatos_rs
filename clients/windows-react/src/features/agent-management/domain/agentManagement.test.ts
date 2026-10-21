import { describe, expect, it } from "vitest";
import { agentAvatarDraftSchema, agentProfileListSchema, heartbeatIntervalLabel } from "./agentManagement";

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

  it("only accepts opaque native avatar previews", () => {
    const draft = {
      id: `reference_${"a".repeat(32)}`,
      name: "avatar.png",
      mimeType: "image/png" as const,
      byteSize: 1_024,
      previewUrl: "chatos-media://localhost/session/reference/presentation/token",
    };
    expect(agentAvatarDraftSchema.parse(draft)).toEqual(draft);
    expect(() => agentAvatarDraftSchema.parse({ ...draft, previewUrl: "file:///Users/me/avatar.png" })).toThrow();
    expect(() => agentAvatarDraftSchema.parse({ ...draft, previewUrl: "data:image/png;base64,AAAA" })).toThrow();
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

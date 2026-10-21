import { describe, expect, it } from "vitest";
import {
  agentAvatarDraftSchema,
  agentProfileListSchema,
  heartbeatIntervalLabel,
  preparedAgentConversationSchema,
} from "./agentManagement";

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

  it("accepts only controlled Agent avatar URLs", () => {
    const preview = "chatos-agent://localhost/11111111-1111-4111-8111-111111111111/agent-1/20";
    expect(agentProfileListSchema.parse([{ ...fixture(), hasAvatar: true, avatarPreviewUrl: preview }])[0]?.avatarPreviewUrl)
      .toBe(preview);
    expect(() => agentProfileListSchema.parse([{
      ...fixture(),
      hasAvatar: true,
      avatarPreviewUrl: "file:///private/avatar.png",
    }])).toThrow();
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

  it("rejects an Agent conversation whose deterministic conversation identity does not match its room", () => {
    expect(() => preparedAgentConversationSchema.parse({
      roomId: "11111111-1111-4111-8111-111111111111",
      agentId: "agent-1",
      conversationId: "agent-direct-22222222-2222-4222-8222-222222222222",
      title: "界面设计师",
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    })).toThrow("Agent 私聊身份不一致");
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
    avatarPreviewUrl: null,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}

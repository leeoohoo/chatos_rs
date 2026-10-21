import { describe, expect, it } from "vitest";
import {
  agentAvatarDraftSchema,
  agentConversationActivitySnapshotSchema,
  agentProfileActivitySnapshotSchema,
  agentProfileListSchema,
  agentStaffingProposalListSchema,
  agentStaffingResolutionSchema,
  agentTeamListSchema,
  agentTeamDetailSchema,
  agentTeamConversationSnapshotSchema,
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

  it("accepts only room-bound and internally consistent Agent Delivery and Run activity", () => {
    const roomId = "11111111-1111-4111-8111-111111111111";
    const snapshot = {
      roomId,
      agentId: "agent-1",
      conversationId: `agent-direct-${roomId}`,
      conversationVersion: 2,
      hasActiveDelivery: false,
      items: [{
        deliveryId: "delivery-1",
        roomId,
        agentId: "agent-1",
        turnId: "turn-1",
        runId: "run-1",
        deliveryStatus: "completed" as const,
        runStatus: "succeeded" as const,
        responseMessageId: "answer-1",
        createdAtUnixMs: 10,
        updatedAtUnixMs: 20,
      }],
    };
    expect(agentConversationActivitySnapshotSchema.parse(snapshot)).toEqual(snapshot);
    expect(() => agentConversationActivitySnapshotSchema.parse({
      ...snapshot,
      hasActiveDelivery: true,
    })).toThrow("Agent 活跃 Delivery 状态不一致");
    expect(() => agentConversationActivitySnapshotSchema.parse({
      ...snapshot,
      items: [{ ...snapshot.items[0], runStatus: "running" }],
    })).toThrow("Agent Delivery 与 Run 状态不一致");
    expect(() => agentConversationActivitySnapshotSchema.parse({
      ...snapshot,
      items: [{ ...snapshot.items[0], agentId: "agent-2" }],
    })).toThrow("Agent 运行动态成员身份不一致");
  });

  it("accepts only agent-bound and internally consistent profile activity", () => {
    const item = {
      deliveryId: "delivery-1",
      roomId: "11111111-1111-4111-8111-111111111111",
      agentId: "agent-1",
      turnId: "turn-1",
      runId: "run-1",
      deliveryStatus: "running" as const,
      runStatus: "running" as const,
      responseMessageId: null,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    };
    const snapshot = { agentId: "agent-1", hasActiveDelivery: true, items: [item] };
    expect(agentProfileActivitySnapshotSchema.parse(snapshot)).toEqual(snapshot);
    expect(() => agentProfileActivitySnapshotSchema.parse({
      ...snapshot,
      agentId: "agent-2",
    })).toThrow("Agent 运行动态成员身份不一致");
    expect(() => agentProfileActivitySnapshotSchema.parse({
      ...snapshot,
      hasActiveDelivery: false,
    })).toThrow("Agent 活跃 Delivery 状态不一致");
    expect(() => agentProfileActivitySnapshotSchema.parse({
      ...snapshot,
      items: [item, { ...item, roomId: "22222222-2222-4222-8222-222222222222" }],
    })).toThrow("Agent 运行动态标识不能重复");
  });

  it("accepts one safe active team projection per project", () => {
    const team = {
      id: "11111111-1111-4111-8111-111111111111",
      projectId: "project-1",
      name: "Alpha Agent 团队",
      goal: "交付 Alpha",
      defaultAgentId: null,
      projectManagerAgentId: null,
      memberCount: 0,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20,
    };
    expect(agentTeamListSchema.parse([team])).toEqual([team]);
    expect(() => agentTeamListSchema.parse([team, {
      ...team,
      id: "22222222-2222-4222-8222-222222222222",
    }])).toThrow("同一项目只能有一个活动 Agent 团队");
    expect(() => agentTeamListSchema.parse([{ ...team, rolePrompt: "private" }])).toThrow();

    const detail = {
      team: { ...team, memberCount: 1, defaultAgentId: "agent-1" },
      members: [{
        agentId: "agent-1",
        role: "设计师",
        responsibility: "负责界面",
        joinedAtUnixMs: 20,
      }],
    };
    expect(agentTeamDetailSchema.parse(detail)).toEqual(detail);
    expect(() => agentTeamDetailSchema.parse({
      ...detail,
      team: { ...detail.team, memberCount: 2 },
    })).toThrow("Agent 团队成员数量不一致");
    expect(() => agentTeamDetailSchema.parse({
      ...detail,
      team: { ...detail.team, projectManagerAgentId: "agent-2" },
    })).toThrow("Agent 团队负责人必须是活动成员");
  });

  it("accepts only kind-consistent pending staffing proposals", () => {
    const sourceRoomId = "11111111-1111-4111-8111-111111111111";
    const creation = {
      id: "proposal-1",
      kind: "agentCreation" as const,
      sourceRoomId,
      proposerAgentId: "agent-1",
      status: "pending" as const,
      name: "后端工程师",
      role: "后端工程师",
      responsibility: "负责服务端交付",
      rolePrompt: "严格按项目目标完成服务端工作",
      thinkingLevel: "high" as const,
      professionKey: "backend_engineer" as const,
      rationale: "团队存在长期后端职责缺口",
      targetTeamId: null,
      targetAgentId: null,
      reason: null,
      handoffPlan: null,
      createdAtUnixMs: 10,
    };
    expect(agentStaffingProposalListSchema.parse([creation])).toEqual([creation]);
    expect(() => agentStaffingProposalListSchema.parse([{
      ...creation,
      kind: "agentRemoval",
      targetAgentId: "agent-2",
      reason: "职责调整",
    }])).toThrow("Agent 人员提案字段不一致");
    expect(() => agentStaffingProposalListSchema.parse([creation, creation])).toThrow("Agent 人员提案不能重复");
  });

  it("requires approved staffing resolutions to describe their real effect", () => {
    const sourceRoomId = "11111111-1111-4111-8111-111111111111";
    const approvedCreation = {
      proposalId: "proposal-1",
      kind: "agentCreation" as const,
      status: "approved" as const,
      sourceRoomId,
      affectedTeamId: null,
      createdAgentId: "agent-created",
    };
    expect(agentStaffingResolutionSchema.parse(approvedCreation)).toEqual(approvedCreation);
    expect(() => agentStaffingResolutionSchema.parse({
      ...approvedCreation,
      kind: "agentRemoval",
    })).toThrow("Agent 人员提案审批结果不一致");
    expect(() => agentStaffingResolutionSchema.parse({
      ...approvedCreation,
      status: "rejected",
    })).toThrow("被拒绝提案不能产生人员变更");
  });

  it("projects bounded team attachment metadata without native paths", () => {
    const teamId = "11111111-1111-4111-8111-111111111111";
    const message = {
      id: "message-1",
      senderKind: "human" as const,
      senderId: "owner-1",
      content: "请检查图片",
      mentionedAgentIds: ["agent-1"],
      attachments: [{
        id: "attachment-1", name: "设计图.png", mimeType: "image/png",
        size: 1_024, kind: "image" as const,
      }],
      createdAtUnixMs: 10,
    };
    expect(agentTeamConversationSnapshotSchema.parse({
      teamId, messages: [message], deliveries: [],
    }).messages[0]?.attachments).toEqual(message.attachments);
    expect(() => agentTeamConversationSnapshotSchema.parse({
      teamId,
      messages: [{ ...message, attachments: [message.attachments[0], message.attachments[0]] }],
      deliveries: [],
    })).toThrow("团队消息附件不能重复");
    expect(() => agentTeamConversationSnapshotSchema.parse({
      teamId,
      messages: [{ ...message, attachments: [{ ...message.attachments[0], path: "C:\\secret.png" }] }],
      deliveries: [],
    })).toThrow();
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

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentTeamTodo } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentTeamTodoStore } from "./agentTeamTodoStore";

describe("agent team todo store", () => {
  beforeEach(() => useAgentTeamTodoStore.getState().reset());

  it("loads, creates, reorders and cancels durable team todos", async () => {
    const port = stubPort();
    await useAgentTeamTodoStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(port.listTeamTodos).toHaveBeenCalledWith("owner-1", TEAM_ID, true);

    expect(await useAgentTeamTodoStore.getState().save(port, {
      todoId: null, agentId: "agent-1", title: "第二个任务", detail: "实现 UI",
      priority: 70, dependencyIds: ["todo-1"], expectedUpdatedAtUnixMs: null,
    })).toBe(true);
    expect(useAgentTeamTodoStore.getState().todos).toHaveLength(2);

    await useAgentTeamTodoStore.getState().move(port, "todo-new", -1);
    expect(port.reorderTeamTodos).toHaveBeenCalledWith(
      "owner-1", TEAM_ID, ["todo-new", "todo-1"],
    );

    const first = useAgentTeamTodoStore.getState().todos.find((todo) => todo.id === "todo-1")!;
    expect(await useAgentTeamTodoStore.getState().cancel(port, first)).toBe(true);
    expect(useAgentTeamTodoStore.getState().todos.find((todo) => todo.id === "todo-1")?.status)
      .toBe("cancelled");
  });

  it("ignores a late response from the previous team", async () => {
    let release!: (value: AgentTeamTodo[]) => void;
    const late = new Promise<AgentTeamTodo[]>((resolve) => { release = resolve; });
    const otherId = "22222222-2222-4222-8222-222222222222";
    const port = stubPort({
      listTeamTodos: vi.fn(async (_owner, teamId) => teamId === TEAM_ID ? late : [todo(otherId)]),
    });
    const stale = useAgentTeamTodoStore.getState().activate(port, "owner-1", TEAM_ID);
    await useAgentTeamTodoStore.getState().activate(port, "owner-1", otherId);
    release([todo(TEAM_ID)]);
    await stale;
    expect(useAgentTeamTodoStore.getState().todos[0]?.teamId).toBe(otherId);
  });
});

const TEAM_ID = "11111111-1111-4111-8111-111111111111";

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => []), loadEditorCatalog: vi.fn(), loadProfileEditor: vi.fn(),
    saveProfile: vi.fn(), pickAvatar: vi.fn(async () => null), loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined), discardAvatarPresentation: vi.fn(async () => undefined),
    listTeamTodos: vi.fn(async () => [todo(TEAM_ID)]),
    createTeamTodo: vi.fn(async (_owner, teamId, input) => ({
      ...todo(teamId), id: "todo-new", title: input.title, detail: input.detail,
      priority: input.priority, dependencyIds: input.dependencyIds,
    })),
    updateTeamTodo: vi.fn(async (_owner, teamId, input) => ({
      ...todo(teamId), id: input.todoId!, title: input.title, updatedAtUnixMs: 21,
    })),
    reorderTeamTodos: vi.fn(async (_owner: string, teamId: string, ids: string[]) => ids.map((id, index) => ({
      ...todo(teamId), id, priority: 100 - index, sortOrder: index,
    }))),
    cancelTeamTodo: vi.fn(async (_owner, teamId, todoId) => ({
      ...todo(teamId), id: todoId, status: "cancelled" as const, updatedAtUnixMs: 21,
    })),
    ...overrides,
  };
}

function todo(teamId: string): AgentTeamTodo {
  return {
    id: "todo-1", agentId: "agent-1", agentName: "工程师", teamId,
    sourceRoomId: null, sourceMessageId: null, title: "第一个任务", detail: "实现底座",
    priority: 80, sortOrder: 0, status: "pending", blockedReason: "", result: "",
    executionContract: {
      objective: "第一个任务", scope: "实现底座", expectedOutputs: ["第一个任务"],
      acceptanceCriteria: ["完成并核验"], constraints: [],
    },
    executionPlan: {
      requiresExecution: true, builtinCapabilities: ["project_read"], plugins: [],
      selectionRevision: "local-v1", selectedAtUnixMs: 10,
    },
    dependencyIds: [], createdAtUnixMs: 10, updatedAtUnixMs: 20,
  };
}

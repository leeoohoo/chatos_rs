import { create } from "zustand";
import type { AgentTeamTodo, SaveAgentTeamTodoInput } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentTeamTodoState {
  ownerUserId: string | null;
  teamId: string | null;
  todos: AgentTeamTodo[];
  isLoading: boolean;
  mutationTodoId: string | null;
  error: string | null;
  sequence: number;
  requestId: number;
  activate(port: AgentManagementPort, ownerUserId: string, teamId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  save(port: AgentManagementPort, input: SaveAgentTeamTodoInput): Promise<boolean>;
  move(port: AgentManagementPort, todoId: string, direction: -1 | 1): Promise<void>;
  cancel(port: AgentManagementPort, todo: AgentTeamTodo): Promise<boolean>;
  reset(): void;
}

export const useAgentTeamTodoStore = create<AgentTeamTodoState>((set, get) => ({
  ownerUserId: null,
  teamId: null,
  todos: [],
  isLoading: false,
  mutationTodoId: null,
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, teamInput) {
    const ownerUserId = ownerInput.trim();
    const teamId = teamInput.trim();
    if (!ownerUserId || !teamId || !port.listTeamTodos) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.teamId !== teamId) {
      set({
        ownerUserId, teamId, todos: [], isLoading: false, mutationTodoId: null,
        error: null, sequence: current.sequence + 1, requestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.requestId !== 0 || !port.listTeamTodos) return;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const requestId = current.sequence + 1;
    set({ sequence: requestId, requestId, isLoading: current.todos.length === 0, error: null });
    try {
      const todos = await port.listTeamTodos(ownerUserId, teamId, true);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      validateTodoList(todos, teamId);
      set({ todos, isLoading: false, requestId: 0 });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      set({ isLoading: false, requestId: 0, error: errorMessage(failure) });
    }
  },

  async save(port, input) {
    const current = get();
    const request = input.todoId ? port.updateTeamTodo : port.createTeamTodo;
    if (!current.ownerUserId || !current.teamId || current.mutationTodoId || !request) return false;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const mutationTodoId = input.todoId ?? "new";
    const sequence = current.sequence;
    set({ mutationTodoId, error: null });
    try {
      const saved = await request.call(port, ownerUserId, teamId, input);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationTodoId !== mutationTodoId) return false;
      if (saved.teamId !== teamId || (input.todoId != null && saved.id !== input.todoId)
        || (input.todoId == null && saved.status !== "pending")) {
        throw new Error("团队任务保存结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        todos: sortTodos([...latest.todos.filter((todo) => todo.id !== saved.id), saved]),
        mutationTodoId: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationTodoId !== mutationTodoId) return false;
      set({ mutationTodoId: null, error: errorMessage(failure) });
      return false;
    }
  },

  async move(port, todoId, direction) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.mutationTodoId || !port.reorderTeamTodos) return;
    const active = current.todos.filter((todo) => todo.status !== "completed" && todo.status !== "cancelled");
    const index = active.findIndex((todo) => todo.id === todoId);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= active.length) return;
    const ordered = [...active];
    [ordered[index], ordered[target]] = [ordered[target]!, ordered[index]!];
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const sequence = current.sequence;
    set({ mutationTodoId: todoId, error: null });
    try {
      const reordered = await port.reorderTeamTodos(ownerUserId, teamId, ordered.map((todo) => todo.id));
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationTodoId !== todoId) return;
      validateTodoList(reordered, teamId);
      const terminal = state.todos.filter((todo) => todo.status === "completed" || todo.status === "cancelled");
      set({ todos: [...reordered, ...terminal], mutationTodoId: null });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationTodoId !== todoId) return;
      set({ mutationTodoId: null, error: errorMessage(failure) });
    }
  },

  async cancel(port, todo) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.mutationTodoId || !port.cancelTeamTodo
      || todo.teamId !== current.teamId || !["pending", "in_progress", "blocked"].includes(todo.status)) return false;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const sequence = current.sequence;
    set({ mutationTodoId: todo.id, error: null });
    try {
      const cancelled = await port.cancelTeamTodo(
        ownerUserId, teamId, todo.id, todo.updatedAtUnixMs,
      );
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationTodoId !== todo.id) return false;
      if (cancelled.id !== todo.id || cancelled.teamId !== teamId
        || !["cancelled", "completed", "blocked"].includes(cancelled.status)) {
        throw new Error("团队任务停止结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        todos: sortTodos([...latest.todos.filter((candidate) => candidate.id !== todo.id), cancelled]),
        mutationTodoId: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationTodoId !== todo.id) return false;
      set({ mutationTodoId: null, error: errorMessage(failure) });
      return false;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null, teamId: null, todos: [], isLoading: false,
      mutationTodoId: null, error: null, sequence: state.sequence + 1, requestId: 0,
    }));
  },
}));

function validateTodoList(todos: AgentTeamTodo[], teamId: string) {
  if (todos.some((todo) => todo.teamId !== teamId)) {
    throw new Error("团队任务身份不一致，请刷新后重试。");
  }
}

function sortTodos(todos: AgentTeamTodo[]) {
  const statusOrder: Record<AgentTeamTodo["status"], number> = {
    in_progress: 0, blocked: 1, pending: 2, completed: 3, cancelled: 4,
  };
  return todos.sort((left, right) => statusOrder[left.status] - statusOrder[right.status]
    || right.priority - left.priority || left.sortOrder - right.sortOrder
    || left.createdAtUnixMs - right.createdAtUnixMs || left.id.localeCompare(right.id));
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "团队任务暂时不可用。";
}

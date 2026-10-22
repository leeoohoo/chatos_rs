import { create } from "zustand";
import type { WeChatBindTicket, WeChatCompanionSnapshot } from "../domain/wechatCompanion";
import type { WeChatCompanionPort } from "../ports/WeChatCompanionPort";

interface WeChatCompanionState {
  ownerUserId: string | null;
  snapshot: WeChatCompanionSnapshot | null;
  ticket: WeChatBindTicket | null;
  isLoading: boolean;
  isMutating: boolean;
  isPolling: boolean;
  pollingDelayMs: number;
  error: string | null;
  notice: string | null;
  epoch: number;
  operation: number;
  activate(port: WeChatCompanionPort, ownerUserId: string): Promise<void>;
  issueTicket(port: WeChatCompanionPort): Promise<void>;
  pollTicket(port: WeChatCompanionPort): Promise<void>;
  confirmTicket(port: WeChatCompanionPort): Promise<void>;
  revokeSession(port: WeChatCompanionPort, sessionId: string): Promise<void>;
  unbind(port: WeChatCompanionPort): Promise<void>;
  clearError(): void;
  reset(): void;
}

export const useWeChatCompanionStore = create<WeChatCompanionState>((set, get) => ({
  ownerUserId: null,
  snapshot: null,
  ticket: null,
  isLoading: false,
  isMutating: false,
  isPolling: false,
  pollingDelayMs: 2_000,
  error: null,
  notice: null,
  epoch: 0,
  operation: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    const epoch = get().epoch + 1;
    set({ ownerUserId, snapshot: null, ticket: null, isLoading: true, isMutating: false, isPolling: false, pollingDelayMs: 2_000, error: null, notice: null, epoch });
    try {
      const snapshot = await port.load(ownerUserId);
      if (current(ownerUserId, epoch, get())) set({ snapshot, isLoading: false });
    } catch (failure) {
      if (current(ownerUserId, epoch, get())) set({ isLoading: false, error: message(failure) });
    }
  },

  async issueTicket(port) {
    const context = beginMutation(get, set);
    if (!context) return;
    try {
      const ticket = await port.issueTicket(context.ownerUserId);
      if (currentOperation(context, get())) set({ ticket, isMutating: false, pollingDelayMs: 2_000 });
    } catch (failure) {
      if (currentOperation(context, get())) set({ isMutating: false, error: message(failure) });
    }
  },

  async pollTicket(port) {
    const state = get();
    if (!state.ownerUserId || !state.ticket || state.ticket.status !== "issued" || state.isPolling) return;
    const ownerUserId = state.ownerUserId;
    const epoch = state.epoch;
    const previewUrl = state.ticket.previewUrl;
    set({ isPolling: true });
    try {
      const ticket = await port.pollTicket(ownerUserId);
      const latest = get();
      if (current(ownerUserId, epoch, latest) && latest.ticket?.previewUrl === previewUrl) {
        set({ ticket, isPolling: false, pollingDelayMs: 2_000, error: null });
      }
    } catch (failure) {
      const latest = get();
      if (current(ownerUserId, epoch, latest) && latest.ticket?.previewUrl === previewUrl) {
        set({ isPolling: false, pollingDelayMs: Math.min(latest.pollingDelayMs * 2, 8_000), error: message(failure) });
      }
    }
  },

  async confirmTicket(port) {
    if (get().ticket?.status !== "claimed") return;
    const context = beginMutation(get, set);
    if (!context) return;
    try {
      await port.confirmTicket(context.ownerUserId);
      const snapshot = await port.load(context.ownerUserId);
      if (currentOperation(context, get())) set({ snapshot, ticket: null, isMutating: false, notice: "微信小程序绑定已确认。" });
    } catch (failure) {
      if (currentOperation(context, get())) set({ isMutating: false, error: message(failure) });
    }
  },

  async revokeSession(port, sessionId) {
    const context = beginMutation(get, set);
    if (!context) return;
    try {
      await port.revokeSession(context.ownerUserId, sessionId);
      const snapshot = await port.load(context.ownerUserId);
      if (currentOperation(context, get())) set({ snapshot, isMutating: false, notice: "已撤销该小程序登录。" });
    } catch (failure) {
      if (currentOperation(context, get())) set({ isMutating: false, error: message(failure) });
    }
  },

  async unbind(port) {
    const context = beginMutation(get, set);
    if (!context) return;
    try {
      await port.unbind(context.ownerUserId);
      const snapshot = await port.load(context.ownerUserId);
      if (currentOperation(context, get())) set({ snapshot, ticket: null, isMutating: false, notice: "微信账号已解除绑定，所有小程序登录已撤销。" });
    } catch (failure) {
      if (currentOperation(context, get())) set({ isMutating: false, error: message(failure) });
    }
  },

  clearError() { set({ error: null }); },
  reset() {
    set((state) => ({ ownerUserId: null, snapshot: null, ticket: null, isLoading: false, isMutating: false, isPolling: false, pollingDelayMs: 2_000, error: null, notice: null, epoch: state.epoch + 1, operation: state.operation + 1 }));
  },
}));

function beginMutation(get: () => WeChatCompanionState, set: (patch: Partial<WeChatCompanionState>) => void) {
  const state = get();
  if (!state.ownerUserId || state.isMutating) return null;
  const context = { ownerUserId: state.ownerUserId, epoch: state.epoch, operation: state.operation + 1 };
  set({ isMutating: true, error: null, notice: null, operation: context.operation });
  return context;
}

function current(ownerUserId: string, epoch: number, state: WeChatCompanionState) {
  return state.ownerUserId === ownerUserId && state.epoch === epoch;
}

function currentOperation(context: { ownerUserId: string; epoch: number; operation: number }, state: WeChatCompanionState) {
  return current(context.ownerUserId, context.epoch, state) && state.operation === context.operation;
}

function message(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}

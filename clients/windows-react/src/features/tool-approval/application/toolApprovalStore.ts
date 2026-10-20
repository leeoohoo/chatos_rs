import { create } from "zustand";
import {
  allowsToolApprovalDecision,
  type ToolApproval,
  type ToolApprovalDecision,
} from "../domain/toolApproval";
import type { ToolApprovalPort } from "../ports/ToolApprovalPort";

interface ToolApprovalState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  approvals: ToolApproval[];
  isLoading: boolean;
  hasLoaded: boolean;
  loadError: string | null;
  requestSequence: number;
  activeRefreshId: number;
  actionIds: Record<string, number>;
  approvalErrors: Record<string, string>;
  activate(port: ToolApprovalPort, ownerUserId: string): Promise<void>;
  suspend(ownerUserId: string): void;
  refresh(port: ToolApprovalPort): Promise<void>;
  decide(
    port: ToolApprovalPort,
    approval: ToolApproval,
    decision: ToolApprovalDecision,
  ): Promise<boolean>;
  clearApprovalError(invocationId: string): void;
  reset(ownerUserId?: string): void;
}

export const useToolApprovalStore = create<ToolApprovalState>((set, get) => ({
  ownerUserId: null,
  hostAvailable: false,
  approvals: [],
  isLoading: false,
  hasLoaded: false,
  loadError: null,
  requestSequence: 0,
  activeRefreshId: 0,
  actionIds: {},
  approvalErrors: {},

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) return;
    const current = get();
    if (current.ownerUserId !== ownerUserId) {
      set({
        ownerUserId,
        hostAvailable: true,
        approvals: [],
        isLoading: false,
        hasLoaded: false,
        loadError: null,
        requestSequence: current.requestSequence + 1,
        activeRefreshId: 0,
        actionIds: {},
        approvalErrors: {},
      });
    } else if (!current.hostAvailable) {
      set({
        hostAvailable: true,
        requestSequence: current.requestSequence + 1,
        activeRefreshId: 0,
        actionIds: {},
      });
    }
    await get().refresh(port);
  },

  suspend(ownerInput) {
    const ownerUserId = ownerInput.trim();
    const current = get();
    if (!ownerUserId || current.ownerUserId !== ownerUserId || !current.hostAvailable) return;
    set({
      hostAvailable: false,
      isLoading: false,
      requestSequence: current.requestSequence + 1,
      activeRefreshId: 0,
      actionIds: {},
    });
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeRefreshId: requestId,
      isLoading: !current.hasLoaded,
      loadError: null,
    });
    try {
      const approvals = await port.listPending(ownerUserId);
      if (!acceptsRefresh(get(), ownerUserId, requestId)) return;
      set((state) => ({
        approvals,
        isLoading: false,
        hasLoaded: true,
        loadError: null,
        activeRefreshId: 0,
        approvalErrors: errorsForPending(state.approvalErrors, approvals),
      }));
    } catch (error) {
      if (!acceptsRefresh(get(), ownerUserId, requestId)) return;
      set({
        isLoading: false,
        hasLoaded: true,
        loadError: errorMessage(error, "审批队列暂时不可用。"),
        activeRefreshId: 0,
      });
    }
  },

  async decide(port, approval, decision) {
    const current = get();
    const durableApproval = current.approvals.find(
      (item) => item.invocationId === approval.invocationId,
    );
    if (!current.ownerUserId
      || !current.hostAvailable
      || !durableApproval
      || durableApproval.invocationVersion !== approval.invocationVersion
      || !allowsToolApprovalDecision(durableApproval, decision)
      || current.actionIds[approval.invocationId] != null) return false;

    const ownerUserId = current.ownerUserId;
    const actionId = current.requestSequence + 1;
    set((state) => {
      const approvalErrors = { ...state.approvalErrors };
      delete approvalErrors[approval.invocationId];
      return {
        requestSequence: actionId,
        activeRefreshId: 0,
        actionIds: { ...state.actionIds, [approval.invocationId]: actionId },
        approvalErrors,
      };
    });

    try {
      const resolution = await port.decide({
        ownerUserId,
        invocationId: approval.invocationId,
        expectedVersion: approval.invocationVersion,
        decision,
      });
      if (!acceptsAction(get(), ownerUserId, approval.invocationId, actionId)) return false;
      if (resolution.invocationId !== approval.invocationId
        || resolution.invocationVersion <= approval.invocationVersion
        || resolution.decision !== decision) {
        throw new Error("Host 返回了不匹配的审批结果。");
      }
      // The mutation response proves Host durably accepted this exact identity/version. Remove
      // only that item, then reconcile the whole queue; never imply that the tool itself ran.
      set((state) => ({
        approvals: state.approvals.filter(
          (item) => item.invocationId !== approval.invocationId,
        ),
      }));
      const reconciliation = await reconcile(port, set, get, ownerUserId, approval.invocationId, actionId);
      if (!acceptsAction(get(), ownerUserId, approval.invocationId, actionId)) return false;
      finishAction(set, approval.invocationId, reconciliation === "failed" ? {
        loadError: "审批已提交，但暂时无法确认最新队列。",
      } : {});
      return true;
    } catch (error) {
      if (!acceptsAction(get(), ownerUserId, approval.invocationId, actionId)) return false;
      const message = errorMessage(error, "审批处理失败。");
      set((state) => ({
        approvalErrors: { ...state.approvalErrors, [approval.invocationId]: message },
      }));
      await reconcile(port, set, get, ownerUserId, approval.invocationId, actionId);
      if (!acceptsAction(get(), ownerUserId, approval.invocationId, actionId)) return false;
      finishAction(set, approval.invocationId);
      return false;
    }
  },

  clearApprovalError(invocationId) {
    set((state) => {
      const approvalErrors = { ...state.approvalErrors };
      delete approvalErrors[invocationId];
      return { approvalErrors };
    });
  },

  reset(ownerInput = "") {
    set((state) => ({
      ownerUserId: ownerInput.trim() || null,
      hostAvailable: false,
      approvals: [],
      isLoading: false,
      hasLoaded: false,
      loadError: null,
      requestSequence: state.requestSequence + 1,
      activeRefreshId: 0,
      actionIds: {},
      approvalErrors: {},
    }));
  },
}));

type SetState = (
  value: Partial<ToolApprovalState>
    | ((state: ToolApprovalState) => Partial<ToolApprovalState>),
) => void;
type GetState = () => ToolApprovalState;

async function reconcile(
  port: ToolApprovalPort,
  set: SetState,
  get: GetState,
  ownerUserId: string,
  invocationId: string,
  actionId: number,
) {
  if (!acceptsAction(get(), ownerUserId, invocationId, actionId)) return "superseded";
  const refreshId = get().requestSequence + 1;
  set({ requestSequence: refreshId, activeRefreshId: refreshId });
  try {
    const approvals = await port.listPending(ownerUserId);
    if (!acceptsAction(get(), ownerUserId, invocationId, actionId)
      || get().activeRefreshId !== refreshId) return "superseded";
    set((state) => ({
      approvals,
      activeRefreshId: 0,
      loadError: null,
      hasLoaded: true,
      approvalErrors: errorsForPending(state.approvalErrors, approvals, invocationId),
    }));
    return "applied";
  } catch {
    if (!acceptsAction(get(), ownerUserId, invocationId, actionId)
      || get().activeRefreshId !== refreshId) return "superseded";
    set({ activeRefreshId: 0 });
    return "failed";
  }
}

function finishAction(set: SetState, invocationId: string, extra: Partial<ToolApprovalState> = {}) {
  set((state) => {
    const actionIds = { ...state.actionIds };
    delete actionIds[invocationId];
    return { ...extra, actionIds };
  });
}

function acceptsRefresh(state: ToolApprovalState, ownerUserId: string, requestId: number) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.activeRefreshId === requestId;
}

function acceptsAction(
  state: ToolApprovalState,
  ownerUserId: string,
  invocationId: string,
  actionId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.actionIds[invocationId] === actionId;
}

function errorsForPending(
  errors: Record<string, string>,
  approvals: ToolApproval[],
  keepInvocationId?: string,
) {
  const pending = new Set(approvals.map((approval) => approval.invocationId));
  if (keepInvocationId) pending.add(keepInvocationId);
  return Object.fromEntries(
    Object.entries(errors).filter(([invocationId]) => pending.has(invocationId)),
  );
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

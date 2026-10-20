import { create } from "zustand";
import type { RemoteConnection } from "../domain/remoteConnection";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";

type DetailOperationKind = "test" | "delete";

interface DetailOperationIdentity {
  id: number;
  kind: DetailOperationKind;
}

interface RemoteConnectionDetailState {
  ownerUserId: string | null;
  presentationId: string | null;
  connection: RemoteConnection | null;
  generation: number;
  requestSequence: number;
  activeOperation: DetailOperationIdentity | null;
  notice: string | null;
  errorMessage: string | null;
  verificationPrompt: string | null;
  verificationCode: string;
  deletedConnectionId: string | null;
  activate(port: RemoteConnectionPort, ownerUserId: string, connection: RemoteConnection): void;
  updateConnection(ownerUserId: string, connection: RemoteConnection): void;
  deactivate(port: RemoteConnectionPort): void;
  setVerificationCode(value: string): void;
  dismissVerification(): void;
  testSavedConnection(port: RemoteConnectionPort, verificationCode?: string | null): Promise<void>;
  deleteConnection(port: RemoteConnectionPort): Promise<string | null>;
  consumeDeletedConnection(): void;
  dismissError(): void;
}

let presentationSequence = 0;

export const useRemoteConnectionDetailStore = create<RemoteConnectionDetailState>((set, get) => ({
  ownerUserId: null,
  presentationId: null,
  connection: null,
  generation: 0,
  requestSequence: 0,
  activeOperation: null,
  notice: null,
  errorMessage: null,
  verificationPrompt: null,
  verificationCode: "",
  deletedConnectionId: null,

  activate(port, ownerInput, connection) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId || !connection.id.trim()) return;
    discardCurrentPresentation(port, get());
    set((state) => ({
      ownerUserId,
      presentationId: `remote-connection-detail-${Date.now()}-${++presentationSequence}`,
      connection,
      generation: state.generation + 1,
      requestSequence: state.requestSequence + 1,
      activeOperation: null,
      notice: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
      deletedConnectionId: null,
    }));
  },

  updateConnection(ownerInput, connection) {
    const ownerUserId = ownerInput.trim();
    const current = get();
    if (!ownerUserId || current.ownerUserId !== ownerUserId
      || current.connection?.id !== connection.id) return;
    set({ connection });
  },

  deactivate(port) {
    const current = get();
    discardCurrentPresentation(port, current);
    set({
      ownerUserId: null,
      presentationId: null,
      connection: null,
      generation: current.generation + 1,
      requestSequence: current.requestSequence + 1,
      activeOperation: null,
      notice: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
      deletedConnectionId: null,
    });
  },

  setVerificationCode(value) {
    if (get().activeOperation) return;
    set({ verificationCode: value, errorMessage: null });
  },

  dismissVerification() {
    if (get().activeOperation) return;
    set({ verificationPrompt: null, verificationCode: "", errorMessage: null });
  },

  async testSavedConnection(port, verificationInput = null) {
    const verificationCode = verificationInput?.trim() || null;
    if (verificationInput != null && verificationCode == null) {
      set({ errorMessage: "请输入验证码。" });
      return;
    }
    const operation = beginOperation(get(), set, "test");
    if (!operation) return;
    const { ownerUserId, connection, generation, identity } = operation;
    try {
      const result = await port.testSaved({
        ownerUserId,
        connectionId: connection.id,
        verificationCode,
      });
      if (!accepts(get(), ownerUserId, connection.id, generation, identity)) return;
      if (result.verificationRequired) {
        set({
          activeOperation: null,
          notice: null,
          verificationPrompt: result.verificationPrompt,
          verificationCode: "",
          errorMessage: verificationCode == null ? null : "验证码未通过，请重新输入。",
        });
      } else {
        set({
          activeOperation: null,
          notice: result.message || "连接测试成功。",
          errorMessage: null,
          verificationPrompt: null,
          verificationCode: "",
        });
      }
    } catch (error) {
      if (!accepts(get(), ownerUserId, connection.id, generation, identity)) return;
      set({ activeOperation: null, notice: null, errorMessage: errorMessage(error) });
    }
  },

  async deleteConnection(port) {
    const operation = beginOperation(get(), set, "delete");
    if (!operation) return null;
    const { ownerUserId, connection, generation, identity } = operation;
    try {
      const deletedConnectionId = await port.delete({
        ownerUserId,
        connectionId: connection.id,
        expectedVersion: connection.version,
      });
      if (!accepts(get(), ownerUserId, connection.id, generation, identity)) return null;
      if (deletedConnectionId !== connection.id) throw new Error("远端连接删除响应不匹配。");
      set({ activeOperation: null, deletedConnectionId, errorMessage: null });
      return deletedConnectionId;
    } catch (error) {
      if (!accepts(get(), ownerUserId, connection.id, generation, identity)) return null;
      set({ activeOperation: null, errorMessage: errorMessage(error) });
      return null;
    }
  },

  consumeDeletedConnection() {
    set({ deletedConnectionId: null });
  },

  dismissError() {
    set({ errorMessage: null });
  },
}));

function beginOperation(
  state: RemoteConnectionDetailState,
  set: (partial: Partial<RemoteConnectionDetailState>) => void,
  kind: DetailOperationKind,
) {
  if (!state.ownerUserId || !state.presentationId || !state.connection || state.activeOperation) {
    return null;
  }
  const identity = { id: state.requestSequence + 1, kind };
  set({
    requestSequence: identity.id,
    activeOperation: identity,
    notice: null,
    errorMessage: null,
  });
  return {
    ownerUserId: state.ownerUserId,
    connection: state.connection,
    generation: state.generation,
    identity,
  };
}

function accepts(
  state: RemoteConnectionDetailState,
  ownerUserId: string,
  connectionId: string,
  generation: number,
  identity: DetailOperationIdentity,
) {
  return state.ownerUserId === ownerUserId
    && state.connection?.id === connectionId
    && state.generation === generation
    && state.activeOperation?.id === identity.id
    && state.activeOperation.kind === identity.kind;
}

function discardCurrentPresentation(port: RemoteConnectionPort, state: RemoteConnectionDetailState) {
  if (!state.ownerUserId || !state.presentationId) return;
  void port.discardCredentialFileSelections({
    ownerUserId: state.ownerUserId,
    presentationId: state.presentationId,
  }).catch(() => undefined);
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "远端连接操作失败。";
}

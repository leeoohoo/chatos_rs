import { create } from "zustand";
import type {
  RemoteConnection,
  RemoteCredentialFileKind,
  RemoteCredentialFileSelection,
} from "../domain/remoteConnection";
import {
  buildRemoteConnectionInput,
  initialRemoteConnectionDraft,
  type RemoteConnectionDraft,
} from "../domain/remoteConnectionDraft";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";

export type RemoteConnectionEditorOperation = "pick" | "test" | "save";

interface OperationIdentity {
  id: number;
  kind: RemoteConnectionEditorOperation;
}

interface RemoteConnectionEditorState {
  ownerUserId: string | null;
  presentationId: string | null;
  editingConnection: RemoteConnection | null;
  connections: RemoteConnection[];
  draft: RemoteConnectionDraft;
  generation: number;
  requestSequence: number;
  activeOperation: OperationIdentity | null;
  successMessage: string | null;
  errorMessage: string | null;
  verificationPrompt: string | null;
  verificationCode: string;
  open(
    port: RemoteConnectionPort,
    ownerUserId: string,
    editingConnection: RemoteConnection | null,
    connections: RemoteConnection[],
  ): void;
  close(port: RemoteConnectionPort): void;
  updateDraft(patch: Partial<RemoteConnectionDraft>): void;
  setVerificationCode(value: string): void;
  dismissVerification(): void;
  pickCredentialFile(port: RemoteConnectionPort, kind: RemoteCredentialFileKind): Promise<void>;
  testConnection(port: RemoteConnectionPort, verificationCode?: string | null): Promise<void>;
  save(port: RemoteConnectionPort): Promise<RemoteConnection | null>;
  dismissError(): void;
}

let presentationSequence = 0;

const emptyDraft = initialRemoteConnectionDraft(null, []);

export const useRemoteConnectionEditorStore = create<RemoteConnectionEditorState>((set, get) => ({
  ownerUserId: null,
  presentationId: null,
  editingConnection: null,
  connections: [],
  draft: emptyDraft,
  generation: 0,
  requestSequence: 0,
  activeOperation: null,
  successMessage: null,
  errorMessage: null,
  verificationPrompt: null,
  verificationCode: "",

  open(port, ownerInput, editingConnection, connections) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) return;
    discardCurrentPresentation(port, get());
    const presentationId = `remote-connection-editor-${Date.now()}-${++presentationSequence}`;
    set((state) => ({
      ownerUserId,
      presentationId,
      editingConnection,
      connections: [...connections],
      draft: initialRemoteConnectionDraft(editingConnection, connections),
      generation: state.generation + 1,
      requestSequence: state.requestSequence + 1,
      activeOperation: null,
      successMessage: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
    }));
  },

  close(port) {
    const current = get();
    discardCurrentPresentation(port, current);
    set({
      ownerUserId: null,
      presentationId: null,
      editingConnection: null,
      connections: [],
      draft: emptyDraft,
      generation: current.generation + 1,
      requestSequence: current.requestSequence + 1,
      activeOperation: null,
      successMessage: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
    });
  },

  updateDraft(patch) {
    if (get().activeOperation) return;
    set((state) => ({
      draft: { ...state.draft, ...patch },
      successMessage: null,
      errorMessage: null,
    }));
  },

  setVerificationCode(value) {
    if (get().activeOperation) return;
    set({ verificationCode: value, errorMessage: null });
  },

  dismissVerification() {
    if (get().activeOperation) return;
    set({ verificationPrompt: null, verificationCode: "", errorMessage: null });
  },

  async pickCredentialFile(port, kind) {
    const operation = beginOperation(get(), set, "pick");
    if (!operation) return;
    const { ownerUserId, presentationId, generation } = operation;
    try {
      const selection = await port.pickCredentialFile({ ownerUserId, presentationId, kind });
      if (!accepts(get(), ownerUserId, presentationId, generation, operation.identity)) return;
      if (selection && selection.kind !== kind) throw new Error("本机文件选择器返回了错误的凭据类型。");
      set((state) => ({
        draft: selection ? applySelection(state.draft, kind, selection) : state.draft,
        activeOperation: null,
        errorMessage: null,
      }));
    } catch (error) {
      if (!accepts(get(), ownerUserId, presentationId, generation, operation.identity)) return;
      set({ activeOperation: null, errorMessage: errorMessage(error) });
    }
  },

  async testConnection(port, verificationInput = null) {
    const current = get();
    const verificationCode = verificationInput?.trim() || null;
    if (verificationInput != null && verificationCode == null) {
      set({ errorMessage: "请输入验证码。" });
      return;
    }
    let input;
    try {
      input = buildRemoteConnectionInput(current.draft, current.editingConnection, current.connections);
    } catch (error) {
      set({ errorMessage: errorMessage(error), successMessage: null });
      return;
    }
    const operation = beginOperation(get(), set, "test");
    if (!operation) return;
    const { ownerUserId, presentationId, generation } = operation;
    try {
      const result = await port.testDraft({
        ownerUserId,
        presentationId,
        input,
        credentialReferenceId: current.editingConnection?.id ?? null,
        verificationCode,
      });
      if (!accepts(get(), ownerUserId, presentationId, generation, operation.identity)) return;
      if (result.verificationRequired) {
        set({
          activeOperation: null,
          verificationPrompt: result.verificationPrompt,
          verificationCode: "",
          successMessage: null,
          errorMessage: verificationCode == null ? null : "验证码未通过，请重新输入。",
        });
      } else {
        set({
          activeOperation: null,
          verificationPrompt: null,
          verificationCode: "",
          successMessage: result.message || (verificationCode == null
            ? "连接测试成功。"
            : "二次验证通过，连接测试成功。"),
          errorMessage: null,
        });
      }
    } catch (error) {
      if (!accepts(get(), ownerUserId, presentationId, generation, operation.identity)) return;
      set({ activeOperation: null, successMessage: null, errorMessage: errorMessage(error) });
    }
  },

  async save(port) {
    const current = get();
    let input;
    try {
      input = buildRemoteConnectionInput(current.draft, current.editingConnection, current.connections);
    } catch (error) {
      set({ errorMessage: errorMessage(error), successMessage: null });
      return null;
    }
    const operation = beginOperation(get(), set, "save");
    if (!operation) return null;
    const { ownerUserId, presentationId, generation } = operation;
    try {
      const connection = current.editingConnection
        ? await port.update({
          ownerUserId,
          presentationId,
          connectionId: current.editingConnection.id,
          expectedVersion: current.editingConnection.version,
          input,
        })
        : await port.create({ ownerUserId, presentationId, input });
      if (!accepts(get(), ownerUserId, presentationId, generation, operation.identity)) return null;
      set({ activeOperation: null, errorMessage: null });
      return connection;
    } catch (error) {
      if (!accepts(get(), ownerUserId, presentationId, generation, operation.identity)) return null;
      set({ activeOperation: null, errorMessage: errorMessage(error) });
      return null;
    }
  },

  dismissError() {
    set({ errorMessage: null });
  },
}));

function beginOperation(
  state: RemoteConnectionEditorState,
  set: (partial: Partial<RemoteConnectionEditorState>) => void,
  kind: RemoteConnectionEditorOperation,
) {
  if (!state.ownerUserId || !state.presentationId || state.activeOperation) return null;
  const identity = { id: state.requestSequence + 1, kind };
  set({
    requestSequence: identity.id,
    activeOperation: identity,
    errorMessage: null,
    successMessage: kind === "pick" ? state.successMessage : null,
  });
  return {
    ownerUserId: state.ownerUserId,
    presentationId: state.presentationId,
    generation: state.generation,
    identity,
  };
}

function accepts(
  state: RemoteConnectionEditorState,
  ownerUserId: string,
  presentationId: string,
  generation: number,
  identity: OperationIdentity,
) {
  return state.ownerUserId === ownerUserId
    && state.presentationId === presentationId
    && state.generation === generation
    && state.activeOperation?.id === identity.id
    && state.activeOperation.kind === identity.kind;
}

function applySelection(
  draft: RemoteConnectionDraft,
  kind: RemoteCredentialFileKind,
  selection: RemoteCredentialFileSelection,
): RemoteConnectionDraft {
  switch (kind) {
    case "private_key": return { ...draft, privateKeySelection: selection };
    case "certificate": return { ...draft, certificateSelection: selection };
    case "jump_private_key": return { ...draft, jumpPrivateKeySelection: selection };
    case "jump_certificate": return { ...draft, jumpCertificateSelection: selection };
  }
}

function discardCurrentPresentation(port: RemoteConnectionPort, state: RemoteConnectionEditorState) {
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

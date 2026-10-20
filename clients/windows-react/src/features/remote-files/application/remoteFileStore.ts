import { create } from "zustand";
import type {
  RemoteDirectoryListing,
  RemoteFileEntry,
  RemoteFileMutationResponse,
  RemoteLocalFileSelection,
} from "../domain/remoteFiles";
import type { RemoteFilePort } from "../ports/RemoteFilePort";

export type RemoteFileOperationKind =
  | "list"
  | "pick_upload"
  | "pick_download"
  | "create"
  | "rename"
  | "delete"
  | "upload"
  | "download"
  | "discard_selections"
  | "cancel_verification";

interface OperationIdentity {
  requestId: number;
  operationId: string;
  kind: RemoteFileOperationKind;
}

type RemoteFilePlan =
  | { kind: "list"; operationId: string; path: string | null }
  | { kind: "create"; operationId: string; parentPath: string; name: string }
  | { kind: "rename"; operationId: string; path: string; newName: string }
  | { kind: "delete"; operationId: string; path: string; recursively: boolean; name: string }
  | { kind: "upload"; operationId: string; selection: RemoteLocalFileSelection; remoteDirectory: string; overwrite: boolean }
  | { kind: "download"; operationId: string; selection: RemoteLocalFileSelection; remotePath: string; overwrite: boolean };

interface PendingDownload {
  selection: RemoteLocalFileSelection;
  remotePath: string;
}

interface RemoteFileState {
  ownerUserId: string | null;
  connectionId: string | null;
  presentationId: string | null;
  generation: number;
  requestSequence: number;
  listing: RemoteDirectoryListing | null;
  selectedRemotePath: string | null;
  selectedUpload: RemoteLocalFileSelection | null;
  pendingDownload: PendingDownload | null;
  searchText: string;
  showsHiddenFiles: boolean;
  activeOperation: OperationIdentity | null;
  notice: string | null;
  refreshWarning: string | null;
  errorMessage: string | null;
  verificationPrompt: string | null;
  verificationCode: string;
  pendingRetry: RemoteFilePlan | null;
  activate(port: RemoteFilePort, ownerUserId: string, connectionId: string): Promise<void>;
  deactivate(port: RemoteFilePort): void;
  setSearchText(value: string): void;
  setShowsHiddenFiles(value: boolean): void;
  selectRemote(path: string | null): void;
  setVerificationCode(value: string): void;
  dismissFeedback(): void;
  loadDirectory(port: RemoteFilePort, path: string | null): Promise<void>;
  pickUploadFile(port: RemoteFilePort): Promise<void>;
  uploadSelected(port: RemoteFilePort, overwrite: boolean): Promise<void>;
  prepareDownload(port: RemoteFilePort, entry: RemoteFileEntry): Promise<boolean>;
  downloadPrepared(port: RemoteFilePort, overwrite: boolean): Promise<void>;
  discardPreparedSelections(port: RemoteFilePort): Promise<void>;
  createDirectory(port: RemoteFilePort, name: string): Promise<void>;
  renameEntry(port: RemoteFilePort, entry: RemoteFileEntry, newName: string): Promise<void>;
  deleteEntry(port: RemoteFilePort, entry: RemoteFileEntry): Promise<void>;
  submitVerification(port: RemoteFilePort): Promise<void>;
  cancelVerification(port: RemoteFilePort): Promise<void>;
}

let presentationSequence = 0;
let activationSequence = 0;
let discardBarrier: Promise<void> = Promise.resolve();

export const useRemoteFileStore = create<RemoteFileState>((set, get) => ({
  ownerUserId: null,
  connectionId: null,
  presentationId: null,
  generation: 0,
  requestSequence: 0,
  listing: null,
  selectedRemotePath: null,
  selectedUpload: null,
  pendingDownload: null,
  searchText: "",
  showsHiddenFiles: false,
  activeOperation: null,
  notice: null,
  refreshWarning: null,
  errorMessage: null,
  verificationPrompt: null,
  verificationCode: "",
  pendingRetry: null,

  async activate(port, ownerInput, connectionInput) {
    const ownerUserId = ownerInput.trim();
    const connectionId = connectionInput.trim();
    if (!ownerUserId || !connectionId) return;
    const activation = ++activationSequence;
    await discardBarrier;
    if (activation !== activationSequence) return;
    const current = get();
    const presentationId = `remote-files-${Date.now()}-${++presentationSequence}`;
    set({
      ownerUserId,
      connectionId,
      presentationId,
      generation: current.generation + 1,
      requestSequence: current.requestSequence + 1,
      listing: null,
      selectedRemotePath: null,
      selectedUpload: null,
      pendingDownload: null,
      searchText: "",
      showsHiddenFiles: false,
      activeOperation: null,
      notice: null,
      refreshWarning: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
      pendingRetry: null,
    });
    await executePlan(port, listPlan(get()), null, get, set);
  },

  deactivate(port) {
    ++activationSequence;
    const current = get();
    if (current.ownerUserId && current.presentationId) {
      const request = { ownerUserId: current.ownerUserId, presentationId: current.presentationId };
      discardBarrier = discardBarrier
        .then(() => port.discardSelections(request))
        .catch(() => undefined);
    }
    set(resetState(current));
  },

  setSearchText(value) { set({ searchText: value }); },
  setShowsHiddenFiles(value) { set({ showsHiddenFiles: value }); },
  selectRemote(path) { if (!get().activeOperation) set({ selectedRemotePath: path }); },
  setVerificationCode(value) { if (!get().activeOperation) set({ verificationCode: value, errorMessage: null }); },
  dismissFeedback() { set({ notice: null, refreshWarning: null, errorMessage: null }); },

  async loadDirectory(port, path) {
    if (get().activeOperation) return;
    await executePlan(port, listPlan(get(), path), null, get, set);
  },

  async pickUploadFile(port) {
    const operation = beginOperation(get(), set, "pick_upload", newOperationId(get()));
    if (!operation) return;
    try {
      const selection = await port.pickUploadFile({
        ownerUserId: operation.ownerUserId,
        presentationId: operation.presentationId,
      });
      if (!accepts(get(), operation)) return;
      set({ activeOperation: null, selectedUpload: selection, notice: null, errorMessage: null });
    } catch (error) {
      failOperation(error, operation, get, set);
    }
  },

  async uploadSelected(port, overwrite) {
    const current = get();
    if (!current.selectedUpload || !current.listing || current.activeOperation) return;
    await executePlan(port, {
      kind: "upload",
      operationId: newOperationId(current),
      selection: current.selectedUpload,
      remoteDirectory: current.listing.path,
      overwrite,
    }, null, get, set);
  },

  async prepareDownload(port, entry) {
    if (entry.kind !== "file") return false;
    const operation = beginOperation(get(), set, "pick_download", newOperationId(get()));
    if (!operation) return false;
    try {
      const selection = await port.pickDownloadDestination({
        ownerUserId: operation.ownerUserId,
        presentationId: operation.presentationId,
        suggestedFileName: entry.name,
      });
      if (!accepts(get(), operation)) return false;
      set({
        activeOperation: null,
        pendingDownload: selection ? { selection, remotePath: entry.path } : null,
        notice: null,
        errorMessage: null,
      });
      return selection != null;
    } catch (error) {
      failOperation(error, operation, get, set);
      return false;
    }
  },

  async downloadPrepared(port, overwrite) {
    const current = get();
    if (!current.pendingDownload || current.activeOperation) return;
    await executePlan(port, {
      kind: "download",
      operationId: newOperationId(current),
      selection: current.pendingDownload.selection,
      remotePath: current.pendingDownload.remotePath,
      overwrite,
    }, null, get, set);
  },

  async discardPreparedSelections(port) {
    const current = get();
    const operation = beginOperation(current, set, "discard_selections", newOperationId(current));
    if (!operation) return;
    try {
      await port.discardSelections({ ownerUserId: operation.ownerUserId, presentationId: operation.presentationId });
      if (!accepts(get(), operation)) return;
      set({ activeOperation: null, selectedUpload: null, pendingDownload: null });
    } catch (error) {
      failOperation(error, operation, get, set);
    }
  },

  async createDirectory(port, nameInput) {
    const current = get();
    const name = nameInput.trim();
    if (!name || !current.listing || current.activeOperation) {
      if (!name) set({ errorMessage: "请输入目录名称。" });
      return;
    }
    await executePlan(port, {
      kind: "create",
      operationId: newOperationId(current),
      parentPath: current.listing.path,
      name,
    }, null, get, set);
  },

  async renameEntry(port, entry, nameInput) {
    const newName = nameInput.trim();
    if (!newName || get().activeOperation) {
      if (!newName) set({ errorMessage: "请输入新名称。" });
      return;
    }
    await executePlan(port, {
      kind: "rename",
      operationId: newOperationId(get()),
      path: entry.path,
      newName,
    }, null, get, set);
  },

  async deleteEntry(port, entry) {
    if (get().activeOperation) return;
    await executePlan(port, {
      kind: "delete",
      operationId: newOperationId(get()),
      path: entry.path,
      recursively: entry.kind === "directory",
      name: entry.name,
    }, null, get, set);
  },

  async submitVerification(port) {
    const current = get();
    const code = current.verificationCode.trim();
    if (!code) {
      set({ errorMessage: "请输入验证码。" });
      return;
    }
    if (!current.pendingRetry || current.activeOperation) return;
    await executePlan(port, current.pendingRetry, code, get, set);
  },

  async cancelVerification(port) {
    const current = get();
    if (!current.verificationPrompt || current.activeOperation) return;
    const operation = beginOperation(current, set, "cancel_verification", newOperationId(current));
    if (!operation) return;
    try {
      await port.discardSelections({
        ownerUserId: operation.ownerUserId,
        presentationId: operation.presentationId,
      });
      if (!accepts(get(), operation)) return;
      set({
        activeOperation: null,
        selectedUpload: null,
        pendingDownload: null,
        pendingRetry: null,
        verificationPrompt: null,
        verificationCode: "",
        errorMessage: "SSH 二次验证已取消。",
      });
    } catch (error) {
      failOperation(error, operation, get, set);
    }
  },
}));

type StoreGet = () => RemoteFileState;
type StoreSet = (partial: Partial<RemoteFileState>) => void;

async function executePlan(
  port: RemoteFilePort,
  plan: RemoteFilePlan,
  verificationCode: string | null,
  get: StoreGet,
  set: StoreSet,
) {
  const operation = beginOperation(get(), set, plan.kind, plan.operationId);
  if (!operation) return;
  const input = { operationId: plan.operationId, connectionId: operation.connectionId, verificationCode };
  try {
    const response = await execute(port, operation, plan, input);
    if (!accepts(get(), operation)) return;
    if (response.status === "verification_required") {
      set({
        activeOperation: null,
        pendingRetry: plan,
        verificationPrompt: response.prompt,
        verificationCode: "",
        errorMessage: verificationCode == null ? null : "验证码未通过，请重新输入。",
      });
      return;
    }
    applyCompleted(plan, response, set);
  } catch (error) {
    failOperation(error, operation, get, set);
  }
}

async function execute(
  port: RemoteFilePort,
  operation: OperationContext,
  plan: RemoteFilePlan,
  input: { operationId: string; connectionId: string; verificationCode: string | null },
) {
  const scope = { ownerUserId: operation.ownerUserId, input };
  switch (plan.kind) {
    case "list": return port.listDirectory({ ...scope, path: plan.path });
    case "create": return port.createDirectory({ ...scope, parentPath: plan.parentPath, name: plan.name });
    case "rename": return port.renameEntry({ ...scope, path: plan.path, newName: plan.newName });
    case "delete": return port.deleteEntry({ ...scope, path: plan.path, recursively: plan.recursively });
    case "upload": return port.uploadFile({ ...scope, presentationId: operation.presentationId, selectionId: plan.selection.selectionId, remoteDirectory: plan.remoteDirectory, overwrite: plan.overwrite });
    case "download": return port.downloadFile({ ...scope, presentationId: operation.presentationId, selectionId: plan.selection.selectionId, remotePath: plan.remotePath, overwrite: plan.overwrite });
  }
}

type CompletedResponse = Awaited<ReturnType<typeof execute>> & { status: "completed" };

function applyCompleted(plan: RemoteFilePlan, response: CompletedResponse, set: StoreSet) {
  const base: Partial<RemoteFileState> = {
    activeOperation: null,
    pendingRetry: null,
    verificationPrompt: null,
    verificationCode: "",
    errorMessage: null,
  };
  if (plan.kind === "list") {
    if ("listing" in response) {
      set({ ...base, listing: response.listing, selectedRemotePath: null, refreshWarning: null });
    }
    return;
  }
  if (plan.kind === "download") {
    if ("fileName" in response) {
      set({ ...base, pendingDownload: null, notice: `已下载 ${response.fileName}`, refreshWarning: null });
    }
    return;
  }
  if (!("result" in response)) return;
  const result = response.result;
  const notice = mutationNotice(plan);
  const update: Partial<RemoteFileState> = {
    ...base,
    notice,
    refreshWarning: result.refreshedListing ? null : (result.message ?? "操作已完成，但目录刷新失败，请手动刷新。"),
  };
  if (result.refreshedListing) {
    update.listing = result.refreshedListing;
    update.selectedRemotePath = null;
  }
  if (plan.kind === "upload") update.selectedUpload = null;
  set(update);
}

function mutationNotice(plan: Exclude<RemoteFilePlan, { kind: "list" | "download" }>) {
  switch (plan.kind) {
    case "create": return `已创建目录 ${plan.name}`;
    case "rename": return `已重命名为 ${plan.newName}`;
    case "delete": return `已删除 ${plan.name}`;
    case "upload": return `已上传 ${plan.selection.fileName}`;
  }
}

interface OperationContext extends OperationIdentity {
  ownerUserId: string;
  connectionId: string;
  presentationId: string;
  generation: number;
}

function beginOperation(state: RemoteFileState, set: StoreSet, kind: RemoteFileOperationKind, operationId: string): OperationContext | null {
  if (!state.ownerUserId || !state.connectionId || !state.presentationId || state.activeOperation) return null;
  const identity = { requestId: state.requestSequence + 1, operationId, kind };
  set({
    requestSequence: identity.requestId,
    activeOperation: identity,
    notice: null,
    refreshWarning: null,
    errorMessage: null,
  });
  return { ...identity, ownerUserId: state.ownerUserId, connectionId: state.connectionId, presentationId: state.presentationId, generation: state.generation };
}

function accepts(state: RemoteFileState, operation: OperationContext) {
  return state.ownerUserId === operation.ownerUserId
    && state.connectionId === operation.connectionId
    && state.presentationId === operation.presentationId
    && state.generation === operation.generation
    && state.activeOperation?.requestId === operation.requestId
    && state.activeOperation.operationId === operation.operationId
    && state.activeOperation.kind === operation.kind;
}

function failOperation(error: unknown, operation: OperationContext, get: StoreGet, set: StoreSet) {
  if (!accepts(get(), operation)) return;
  set({ activeOperation: null, errorMessage: errorMessage(error) });
}

function listPlan(state: RemoteFileState, path: string | null = null): RemoteFilePlan {
  return { kind: "list", operationId: newOperationId(state), path };
}

function newOperationId(state: Pick<RemoteFileState, "requestSequence">) {
  return `remote-file-${Date.now()}-${state.requestSequence + 1}`;
}

function resetState(state: RemoteFileState): Partial<RemoteFileState> {
  return {
    ownerUserId: null,
    connectionId: null,
    presentationId: null,
    generation: state.generation + 1,
    requestSequence: state.requestSequence + 1,
    listing: null,
    selectedRemotePath: null,
    selectedUpload: null,
    pendingDownload: null,
    searchText: "",
    showsHiddenFiles: false,
    activeOperation: null,
    notice: null,
    refreshWarning: null,
    errorMessage: null,
    verificationPrompt: null,
    verificationCode: "",
    pendingRetry: null,
  };
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "远端文件操作失败。";
}

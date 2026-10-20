import { create } from "zustand";
import {
  generatedAssetPreviewUrl,
  type MediaStudioGeneratedAsset,
  type MediaStudioGenerationResult,
  type MediaStudioHistoryAsset,
  type MediaStudioHistoryCursor,
  type MediaStudioHistoryItem,
  type MediaStudioModel,
  type MediaStudioReference,
} from "../domain/mediaStudio";
import type {
  MediaStudioHistoryReference,
  MediaStudioPort,
} from "../ports/MediaStudioPort";

export interface MediaStudioTransientItem {
  source: "generatedWithoutPersistence";
  id: string;
  prompt: string;
  modelName: string;
  createdAtUnixMs: number;
  images: MediaStudioGeneratedAsset[];
}

export type MediaStudioDisplayItem =
  | { source: "history"; item: MediaStudioHistoryItem }
  | MediaStudioTransientItem;

interface OperationIdentity {
  ownerUserId: string;
  accountGeneration: number;
  presentationId: string;
  presentationGeneration: number;
  requestId: number;
}

interface MediaStudioState {
  ownerUserId: string | null;
  accountGeneration: number;
  presentationId: string | null;
  presentationGeneration: number;
  requestSequence: number;
  models: MediaStudioModel[];
  selectedModelId: string | null;
  isLoadingModels: boolean;
  hasLoadedModels: boolean;
  modelsErrorMessage: string | null;
  activeModelsRequest: number | null;
  history: MediaStudioHistoryItem[];
  transientItems: MediaStudioTransientItem[];
  historyCursor: MediaStudioHistoryCursor | null;
  unreadableHistoryCount: number;
  isLoadingHistory: boolean;
  hasLoadedHistory: boolean;
  isLoadingOlderHistory: boolean;
  historyErrorMessage: string | null;
  activeHistoryRequest: number | null;
  activeOlderHistoryRequest: number | null;
  prompt: string;
  size: string;
  count: number;
  references: MediaStudioReference[];
  isImportingReferences: boolean;
  referenceErrorMessages: string[];
  activeReferenceRequest: number | null;
  isGenerating: boolean;
  activeGenerationRequest: number | null;
  generationErrorMessage: string | null;
  persistenceWarning: string | null;
  activate(port: MediaStudioPort, ownerUserId: string): Promise<void>;
  refreshModels(port: MediaStudioPort): Promise<void>;
  refreshHistory(port: MediaStudioPort): Promise<void>;
  loadOlderHistory(port: MediaStudioPort): Promise<void>;
  setSelectedModel(modelId: string): void;
  setPrompt(prompt: string): void;
  setSize(size: string): void;
  setCount(count: number): void;
  pickReferences(port: MediaStudioPort): Promise<void>;
  addHistoryReferences(
    port: MediaStudioPort,
    assets: MediaStudioHistoryReference[],
  ): Promise<void>;
  removeReference(port: MediaStudioPort, referenceId: string): Promise<void>;
  generate(port: MediaStudioPort): Promise<void>;
  dismissGenerationError(): void;
  dismissHistoryFeedback(): void;
  dismissReferenceErrors(): void;
  deactivate(port: MediaStudioPort): void;
  reset(port: MediaStudioPort): void;
}

type StoreGet = () => MediaStudioState;
type StoreSet = (
  partial: Partial<MediaStudioState>
    | ((state: MediaStudioState) => Partial<MediaStudioState>),
) => void;

let activationSequence = 0;
let fallbackUuidSequence = 0;
let discardBarrier: Promise<void> = Promise.resolve();

export const useMediaStudioStore = create<MediaStudioState>((set, get) => ({
  ...emptyState(),
  accountGeneration: 0,
  presentationGeneration: 0,
  requestSequence: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset(port);
      return;
    }
    const activation = ++activationSequence;
    const current = get();
    if (current.ownerUserId === ownerUserId && current.presentationId) {
      if (!current.hasLoadedModels && !current.isLoadingModels) void get().refreshModels(port);
      if (!current.hasLoadedHistory && !current.isLoadingHistory) void get().refreshHistory(port);
      return;
    }
    queuePresentationDiscard(port, current);
    await discardBarrier;
    if (activation !== activationSequence) return;
    const latest = get();
    set({
      ...emptyState(),
      ownerUserId,
      accountGeneration: latest.accountGeneration + 1,
      presentationId: newUuid(),
      presentationGeneration: latest.presentationGeneration + 1,
      requestSequence: latest.requestSequence + 1,
    });
    await Promise.all([get().refreshModels(port), get().refreshHistory(port)]);
  },

  async refreshModels(port) {
    const identity = beginModelsRequest(get, set);
    if (!identity) return;
    try {
      const models = await port.listModels(identity.ownerUserId);
      if (!acceptsModels(get(), identity)) return;
      const available = models.filter((model) => model.enabled && model.hasApiKey);
      const selectedModelId = available.some((model) => model.id === get().selectedModelId)
        ? get().selectedModelId
        : available[0]?.id ?? null;
      set({
        models: available,
        selectedModelId,
        isLoadingModels: false,
        hasLoadedModels: true,
        modelsErrorMessage: null,
        activeModelsRequest: null,
      });
    } catch (error) {
      if (!acceptsModels(get(), identity)) return;
      set({
        isLoadingModels: false,
        hasLoadedModels: true,
        modelsErrorMessage: errorMessage(error, "图片模型暂时无法读取。"),
        activeModelsRequest: null,
      });
    }
  },

  async refreshHistory(port) {
    const identity = beginHistoryRequest(get, set, false);
    if (!identity) return;
    try {
      const page = await port.loadImageHistory({ ownerUserId: identity.ownerUserId, limit: 10 });
      if (!acceptsInitialHistory(get(), identity)) return;
      set({
        history: sortedHistory(page.items),
        historyCursor: page.nextCursor,
        unreadableHistoryCount: page.unreadableCount,
        isLoadingHistory: false,
        hasLoadedHistory: true,
        historyErrorMessage: unreadableHistoryMessage(page.unreadableCount),
        activeHistoryRequest: null,
      });
    } catch (error) {
      if (!acceptsInitialHistory(get(), identity)) return;
      set({
        isLoadingHistory: false,
        hasLoadedHistory: true,
        historyErrorMessage: `读取创作记录失败：${errorMessage(error, "未知错误")}`,
        activeHistoryRequest: null,
      });
    }
  },

  async loadOlderHistory(port) {
    const identity = beginHistoryRequest(get, set, true);
    if (!identity) return;
    const cursor = get().historyCursor;
    if (!cursor) return;
    try {
      const page = await port.loadImageHistory({
        ownerUserId: identity.ownerUserId,
        after: cursor,
        limit: 10,
      });
      if (!acceptsOlderHistory(get(), identity)) return;
      const unreadableHistoryCount = get().unreadableHistoryCount + page.unreadableCount;
      set({
        history: mergeHistory(get().history, page.items),
        historyCursor: page.nextCursor,
        unreadableHistoryCount,
        isLoadingOlderHistory: false,
        historyErrorMessage: unreadableHistoryMessage(unreadableHistoryCount),
        activeOlderHistoryRequest: null,
      });
    } catch (error) {
      if (!acceptsOlderHistory(get(), identity)) return;
      set({
        isLoadingOlderHistory: false,
        historyErrorMessage: `读取更多创作记录失败：${errorMessage(error, "未知错误")}`,
        activeOlderHistoryRequest: null,
      });
    }
  },

  setSelectedModel(modelId) {
    const state = get();
    if (state.isGenerating || !state.models.some((model) => model.id === modelId)) return;
    set({ selectedModelId: modelId, generationErrorMessage: null });
  },

  setPrompt(prompt) {
    if (!get().isGenerating) set({ prompt, generationErrorMessage: null });
  },

  setSize(size) {
    if (!get().isGenerating && supportedImageSizes.some((candidate) => candidate === size)) {
      set({ size, generationErrorMessage: null });
    }
  },

  setCount(count) {
    if (!get().isGenerating && Number.isInteger(count) && count >= 1 && count <= 10) {
      set({ count, generationErrorMessage: null });
    }
  },

  async pickReferences(port) {
    const state = get();
    if (state.isGenerating || state.references.length >= 8) {
      if (state.references.length >= 8) set({ referenceErrorMessages: [tooManyReferencesMessage] });
      return;
    }
    const identity = beginReferenceRequest(get, set);
    if (!identity) return;
    try {
      const result = await port.pickReferenceImages(referenceScope(identity));
      if (!acceptsReferences(get(), identity)) {
        discardLateReferences(port, identity, result.references);
        return;
      }
      applyReferenceImport(result.references, result.errors, get, set);
    } catch (error) {
      failReferenceRequest(error, identity, get, set);
    }
  },

  async addHistoryReferences(port, assets) {
    const state = get();
    if (state.isGenerating || assets.length === 0) return;
    const authorized = authorizedHistoryReferences(state.history);
    const uniqueAssets = uniqueHistoryReferences(assets);
    if (uniqueAssets.length !== assets.length
      || uniqueAssets.some((asset) => !authorized.has(historyReferenceKey(asset)))) {
      set({ referenceErrorMessages: ["只能使用当前账号图片历史中的资源。"] });
      return;
    }
    if (state.references.length + uniqueAssets.length > 8) {
      set({ referenceErrorMessages: [tooManyReferencesMessage] });
      return;
    }
    const identity = beginReferenceRequest(get, set);
    if (!identity) return;
    try {
      const result = await port.stageHistoryReferences({
        ...referenceScope(identity),
        assets: uniqueAssets,
      });
      if (!acceptsReferences(get(), identity)) {
        discardLateReferences(port, identity, result.references);
        return;
      }
      applyReferenceImport(result.references, result.errors, get, set);
    } catch (error) {
      failReferenceRequest(error, identity, get, set);
    }
  },

  async removeReference(port, referenceId) {
    const state = get();
    if (state.isGenerating || !state.references.some((reference) => reference.id === referenceId)) return;
    const identity = beginReferenceRequest(get, set);
    if (!identity) return;
    try {
      await port.discardReferenceImages({ ...referenceScope(identity), referenceIds: [referenceId] });
      if (!acceptsReferences(get(), identity)) return;
      set({
        references: get().references.filter((reference) => reference.id !== referenceId),
        isImportingReferences: false,
        activeReferenceRequest: null,
        referenceErrorMessages: [],
      });
    } catch (error) {
      failReferenceRequest(error, identity, get, set);
    }
  },

  async generate(port) {
    const identity = beginGeneration(get, set);
    if (!identity) return;
    const state = get();
    const prompt = state.prompt.trim();
    const selectedModelId = state.selectedModelId!;
    const clientRequestId = newUuid();
    try {
      const outcome = await port.generateImage({
        ownerUserId: identity.ownerUserId,
        request: {
          presentationId: identity.presentationId,
          modelConfigId: selectedModelId,
          prompt,
          size: state.size === "auto" ? null : state.size,
          count: state.count,
          referenceImageIds: state.references.map((reference) => reference.id),
          clientRequestId,
        },
      });
      if (!acceptsGeneration(get(), identity)) return;
      if (outcome.status === "saved") {
        set({
          history: mergeHistory([outcome.item], get().history),
          transientItems: get().transientItems.filter((item) => item.id !== outcome.item.id),
          isGenerating: false,
          activeGenerationRequest: null,
          generationErrorMessage: null,
          persistenceWarning: null,
        });
      } else {
        const transient: MediaStudioTransientItem = {
          source: "generatedWithoutPersistence",
          id: outcome.result.id,
          prompt,
          modelName: outcome.result.modelName,
          createdAtUnixMs: outcome.result.createdAtUnixMs,
          images: outcome.result.images,
        };
        const warning = `图片已生成，但未能保存到本机：${outcome.persistenceErrorMessage}`;
        set({
          transientItems: mergeTransient([transient], get().transientItems),
          isGenerating: false,
          activeGenerationRequest: null,
          generationErrorMessage: warning,
          persistenceWarning: warning,
        });
      }
    } catch (error) {
      if (!acceptsGeneration(get(), identity)) return;
      set({
        isGenerating: false,
        activeGenerationRequest: null,
        generationErrorMessage: errorMessage(error, "图片生成失败。"),
      });
    }
  },

  dismissGenerationError() { set({ generationErrorMessage: null }); },
  dismissHistoryFeedback() { set({ historyErrorMessage: null, persistenceWarning: null }); },
  dismissReferenceErrors() { set({ referenceErrorMessages: [] }); },

  deactivate(port) {
    ++activationSequence;
    const state = get();
    queuePresentationDiscard(port, state);
    set(resetState(state));
  },

  reset(port) {
    ++activationSequence;
    const state = get();
    queuePresentationDiscard(port, state);
    set(resetState(state));
  },
}));

export function canGenerateMediaStudioImage(state: MediaStudioState) {
  return state.ownerUserId != null
    && state.presentationId != null
    && !state.isGenerating
    && !state.isLoadingHistory
    && !state.isImportingReferences
    && state.selectedModelId != null
    && state.models.some((model) => model.id === state.selectedModelId)
    && state.prompt.trim().length > 0;
}

export function mediaStudioDisplayItems(state: MediaStudioState): MediaStudioDisplayItem[] {
  return [
    ...state.history.map((item) => ({ source: "history" as const, item })),
    ...state.transientItems,
  ].sort((left, right) => {
    const leftTime = left.source === "history" ? left.item.createdAtUnixMs : left.createdAtUnixMs;
    const rightTime = right.source === "history" ? right.item.createdAtUnixMs : right.createdAtUnixMs;
    const leftId = left.source === "history" ? left.item.id : left.id;
    const rightId = right.source === "history" ? right.item.id : right.id;
    return rightTime - leftTime || rightId.localeCompare(leftId);
  });
}

export function mediaStudioAssetPreviewUrl(asset: MediaStudioHistoryAsset | MediaStudioGeneratedAsset) {
  return "contentToken" in asset ? asset.previewUrl : generatedAssetPreviewUrl(asset);
}

export const supportedImageSizes = ["auto", "1024x1024", "1536x1024", "1024x1536"] as const;

function emptyState() {
  return {
    ownerUserId: null,
    presentationId: null,
    models: [] as MediaStudioModel[],
    selectedModelId: null,
    isLoadingModels: false,
    hasLoadedModels: false,
    modelsErrorMessage: null,
    activeModelsRequest: null,
    history: [] as MediaStudioHistoryItem[],
    transientItems: [] as MediaStudioTransientItem[],
    historyCursor: null,
    unreadableHistoryCount: 0,
    isLoadingHistory: false,
    hasLoadedHistory: false,
    isLoadingOlderHistory: false,
    historyErrorMessage: null,
    activeHistoryRequest: null,
    activeOlderHistoryRequest: null,
    prompt: "",
    size: "1024x1024",
    count: 1,
    references: [] as MediaStudioReference[],
    isImportingReferences: false,
    referenceErrorMessages: [] as string[],
    activeReferenceRequest: null,
    isGenerating: false,
    activeGenerationRequest: null,
    generationErrorMessage: null,
    persistenceWarning: null,
  };
}

function resetState(state: MediaStudioState): Partial<MediaStudioState> {
  return {
    ...emptyState(),
    accountGeneration: state.accountGeneration + 1,
    presentationGeneration: state.presentationGeneration + 1,
    requestSequence: state.requestSequence + 1,
  };
}

function beginModelsRequest(get: StoreGet, set: StoreSet): OperationIdentity | null {
  const state = get();
  const identity = operationIdentity(state);
  if (!identity) return null;
  set({
    requestSequence: identity.requestId,
    activeModelsRequest: identity.requestId,
    isLoadingModels: true,
    modelsErrorMessage: null,
  });
  return identity;
}

function beginHistoryRequest(
  get: StoreGet,
  set: StoreSet,
  older: boolean,
): OperationIdentity | null {
  const state = get();
  if (state.isGenerating || state.isLoadingOlderHistory || (older && state.isLoadingHistory)) return null;
  if (older && !state.historyCursor) return null;
  const identity = operationIdentity(state);
  if (!identity) return null;
  if (older) {
    set({
      requestSequence: identity.requestId,
      activeOlderHistoryRequest: identity.requestId,
      isLoadingOlderHistory: true,
      historyErrorMessage: unreadableHistoryMessage(state.unreadableHistoryCount),
    });
  } else {
    set({
      requestSequence: identity.requestId,
      activeHistoryRequest: identity.requestId,
      isLoadingHistory: true,
      historyErrorMessage: null,
    });
  }
  return identity;
}

function beginReferenceRequest(get: StoreGet, set: StoreSet): OperationIdentity | null {
  const state = get();
  if (state.isGenerating) return null;
  const identity = operationIdentity(state);
  if (!identity) return null;
  set({
    requestSequence: identity.requestId,
    activeReferenceRequest: identity.requestId,
    isImportingReferences: true,
    referenceErrorMessages: [],
  });
  return identity;
}

function beginGeneration(get: StoreGet, set: StoreSet): OperationIdentity | null {
  const state = get();
  if (!canGenerateMediaStudioImage(state)) return null;
  const identity = operationIdentity(state);
  if (!identity) return null;
  set({
    requestSequence: identity.requestId,
    activeGenerationRequest: identity.requestId,
    isGenerating: true,
    generationErrorMessage: null,
    persistenceWarning: null,
  });
  return identity;
}

function operationIdentity(state: MediaStudioState): OperationIdentity | null {
  if (!state.ownerUserId || !state.presentationId) return null;
  return {
    ownerUserId: state.ownerUserId,
    accountGeneration: state.accountGeneration,
    presentationId: state.presentationId,
    presentationGeneration: state.presentationGeneration,
    requestId: state.requestSequence + 1,
  };
}

function acceptsBase(state: MediaStudioState, identity: OperationIdentity) {
  return state.ownerUserId === identity.ownerUserId
    && state.accountGeneration === identity.accountGeneration
    && state.presentationId === identity.presentationId
    && state.presentationGeneration === identity.presentationGeneration;
}

function acceptsModels(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeModelsRequest === identity.requestId;
}

function acceptsInitialHistory(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeHistoryRequest === identity.requestId;
}

function acceptsOlderHistory(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeOlderHistoryRequest === identity.requestId;
}

function acceptsReferences(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeReferenceRequest === identity.requestId;
}

function acceptsGeneration(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeGenerationRequest === identity.requestId;
}

function referenceScope(identity: OperationIdentity) {
  return { ownerUserId: identity.ownerUserId, presentationId: identity.presentationId };
}

function applyReferenceImport(
  imported: MediaStudioReference[],
  errors: string[],
  get: StoreGet,
  set: StoreSet,
) {
  const references = mergeReferences(get().references, imported);
  const overflow = references.length > 8;
  set({
    references: references.slice(0, 8),
    isImportingReferences: false,
    activeReferenceRequest: null,
    referenceErrorMessages: overflow ? [...errors, tooManyReferencesMessage] : errors,
  });
}

function failReferenceRequest(
  error: unknown,
  identity: OperationIdentity,
  get: StoreGet,
  set: StoreSet,
) {
  if (!acceptsReferences(get(), identity)) return;
  set({
    isImportingReferences: false,
    activeReferenceRequest: null,
    referenceErrorMessages: [errorMessage(error, "参考图暂时无法读取。")],
  });
}

function discardLateReferences(
  port: MediaStudioPort,
  identity: OperationIdentity,
  references: MediaStudioReference[],
) {
  if (references.length === 0) return;
  void port.discardReferenceImages({
    ...referenceScope(identity),
    referenceIds: references.map((reference) => reference.id),
  }).catch(() => undefined);
}

function mergeReferences(current: MediaStudioReference[], incoming: MediaStudioReference[]) {
  const values = [...current];
  const ids = new Set(values.map((reference) => reference.id));
  for (const reference of incoming) {
    if (!ids.has(reference.id)) {
      values.push(reference);
      ids.add(reference.id);
    }
  }
  return values;
}

function mergeHistory(current: MediaStudioHistoryItem[], incoming: MediaStudioHistoryItem[]) {
  const values = new Map<string, MediaStudioHistoryItem>();
  for (const item of [...current, ...incoming]) {
    if (!values.has(item.id)) values.set(item.id, item);
  }
  return sortedHistory([...values.values()]);
}

function sortedHistory(items: MediaStudioHistoryItem[]) {
  return [...items].sort(
    (left, right) => right.createdAtUnixMs - left.createdAtUnixMs || right.id.localeCompare(left.id),
  );
}

function mergeTransient(current: MediaStudioTransientItem[], incoming: MediaStudioTransientItem[]) {
  const values = new Map<string, MediaStudioTransientItem>();
  for (const item of [...current, ...incoming]) {
    if (!values.has(item.id)) values.set(item.id, item);
  }
  return [...values.values()].sort(
    (left, right) => right.createdAtUnixMs - left.createdAtUnixMs || right.id.localeCompare(left.id),
  );
}

function authorizedHistoryReferences(history: MediaStudioHistoryItem[]) {
  return new Set(history.flatMap((item) => item.images.map(
    (asset) => historyReferenceKey({ recordId: item.id, contentToken: asset.contentToken }),
  )));
}

function uniqueHistoryReferences(assets: MediaStudioHistoryReference[]) {
  const values = new Map<string, MediaStudioHistoryReference>();
  for (const asset of assets) values.set(historyReferenceKey(asset), asset);
  return [...values.values()];
}

function historyReferenceKey(asset: MediaStudioHistoryReference) {
  return `${asset.recordId}:${asset.contentToken}`;
}

function unreadableHistoryMessage(count: number) {
  return count > 0 ? `有 ${count} 条记录无法读取，原文件已保留。` : null;
}

function queuePresentationDiscard(port: MediaStudioPort, state: MediaStudioState) {
  if (!state.ownerUserId || !state.presentationId) return;
  const scope = { ownerUserId: state.ownerUserId, presentationId: state.presentationId };
  discardBarrier = discardBarrier
    .then(() => port.discardPresentation(scope))
    .catch(() => undefined);
}

function newUuid() {
  const random = globalThis.crypto?.randomUUID?.();
  if (random) return random.toLowerCase();
  fallbackUuidSequence += 1;
  const tail = `${Date.now().toString(16)}${fallbackUuidSequence.toString(16)}`
    .slice(-12)
    .padStart(12, "0");
  return `00000000-0000-4000-8000-${tail}`;
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

const tooManyReferencesMessage = "参考图最多只能添加 8 张。";

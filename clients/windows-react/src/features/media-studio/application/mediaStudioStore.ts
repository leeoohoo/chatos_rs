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
  type MediaStudioVideoAudioDraft,
  type MediaStudioVideoHistoryCursor,
  type MediaStudioVideoHistoryItem,
  type MediaStudioVideoJobSnapshot,
  type MediaStudioVideoJobStatus,
  type MediaStudioVideoRatio,
  type MediaStudioVideoSize,
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
  videoModels: MediaStudioModel[];
  selectedVideoModelId: string | null;
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
  videoHistory: MediaStudioVideoHistoryItem[];
  videoHistoryCursor: MediaStudioVideoHistoryCursor | null;
  unreadableVideoHistoryCount: number;
  isLoadingVideoHistory: boolean;
  hasLoadedVideoHistory: boolean;
  isLoadingOlderVideoHistory: boolean;
  videoHistoryErrorMessage: string | null;
  activeVideoHistoryRequest: number | null;
  activeOlderVideoHistoryRequest: number | null;
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
  videoPrompt: string;
  videoSize: MediaStudioVideoSize;
  videoSeconds: number;
  videoRatio: MediaStudioVideoRatio;
  videoInputImage: MediaStudioReference | null;
  videoReferenceAudio: MediaStudioVideoAudioDraft | null;
  isImportingVideoInput: boolean;
  isImportingVideoAudio: boolean;
  videoInputErrorMessage: string | null;
  activeVideoInputRequest: number | null;
  activeVideoAudioRequest: number | null;
  isGeneratingVideo: boolean;
  videoJob: MediaStudioVideoJobSnapshot | null;
  videoGenerationErrorMessage: string | null;
  activeVideoGenerationRequest: number | null;
  activeVideoPollRequest: number | null;
  activeVideoClientRequestId: string | null;
  activate(port: MediaStudioPort, ownerUserId: string): Promise<void>;
  refreshModels(port: MediaStudioPort): Promise<void>;
  refreshHistory(port: MediaStudioPort): Promise<void>;
  loadOlderHistory(port: MediaStudioPort): Promise<void>;
  refreshVideoHistory(port: MediaStudioPort): Promise<void>;
  loadOlderVideoHistory(port: MediaStudioPort): Promise<void>;
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
  setSelectedVideoModel(modelId: string): void;
  setVideoPrompt(prompt: string): void;
  setVideoSize(size: MediaStudioVideoSize): void;
  setVideoSeconds(seconds: number): void;
  setVideoRatio(ratio: MediaStudioVideoRatio): void;
  pickVideoInputImage(port: MediaStudioPort): Promise<void>;
  addHistoryVideoInput(port: MediaStudioPort, asset: MediaStudioHistoryReference): Promise<void>;
  removeVideoInputImage(port: MediaStudioPort): Promise<void>;
  pickVideoReferenceAudio(port: MediaStudioPort): Promise<void>;
  removeVideoReferenceAudio(port: MediaStudioPort): Promise<void>;
  generateVideo(port: MediaStudioPort): Promise<void>;
  pollVideo(port: MediaStudioPort): Promise<void>;
  cancelVideo(port: MediaStudioPort): Promise<void>;
  dismissVideoError(): void;
  dismissVideoHistoryFeedback(): void;
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
let videoPollTimer: ReturnType<typeof setTimeout> | null = null;

export const mediaStudioVideoPollIntervalMs = 10_000;

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
      if (!current.hasLoadedVideoHistory && !current.isLoadingVideoHistory) {
        void get().refreshVideoHistory(port);
      }
      return;
    }
    clearVideoPollTimer();
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
    await Promise.all([
      get().refreshModels(port),
      get().refreshHistory(port),
      get().refreshVideoHistory(port),
    ]);
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
      const videoModels = available.filter(isSupportedVideoModel);
      const selectedVideoModelId = videoModels.some(
        (model) => model.id === get().selectedVideoModelId,
      ) ? get().selectedVideoModelId : videoModels[0]?.id ?? null;
      set({
        models: available,
        selectedModelId,
        videoModels,
        selectedVideoModelId,
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

  async refreshVideoHistory(port) {
    const identity = beginVideoHistoryRequest(get, set, false);
    if (!identity) return;
    try {
      const page = await port.loadVideoHistory({ ownerUserId: identity.ownerUserId, limit: 10 });
      if (!acceptsInitialVideoHistory(get(), identity)) return;
      set({
        videoHistory: sortedVideoHistory(page.items),
        videoHistoryCursor: page.nextCursor,
        unreadableVideoHistoryCount: page.unreadableCount,
        isLoadingVideoHistory: false,
        hasLoadedVideoHistory: true,
        videoHistoryErrorMessage: unreadableVideoHistoryMessage(page.unreadableCount),
        activeVideoHistoryRequest: null,
      });
    } catch (error) {
      if (!acceptsInitialVideoHistory(get(), identity)) return;
      set({
        isLoadingVideoHistory: false,
        hasLoadedVideoHistory: true,
        videoHistoryErrorMessage: `读取视频记录失败：${errorMessage(error, "未知错误")}`,
        activeVideoHistoryRequest: null,
      });
    }
  },

  async loadOlderVideoHistory(port) {
    const identity = beginVideoHistoryRequest(get, set, true);
    if (!identity) return;
    const cursor = get().videoHistoryCursor;
    if (!cursor) return;
    try {
      const page = await port.loadVideoHistory({
        ownerUserId: identity.ownerUserId,
        after: cursor,
        limit: 10,
      });
      if (!acceptsOlderVideoHistory(get(), identity)) return;
      const unreadableVideoHistoryCount = get().unreadableVideoHistoryCount + page.unreadableCount;
      set({
        videoHistory: mergeVideoHistory(get().videoHistory, page.items),
        videoHistoryCursor: page.nextCursor,
        unreadableVideoHistoryCount,
        isLoadingOlderVideoHistory: false,
        videoHistoryErrorMessage: unreadableVideoHistoryMessage(unreadableVideoHistoryCount),
        activeOlderVideoHistoryRequest: null,
      });
    } catch (error) {
      if (!acceptsOlderVideoHistory(get(), identity)) return;
      set({
        isLoadingOlderVideoHistory: false,
        videoHistoryErrorMessage: `读取更多视频记录失败：${errorMessage(error, "未知错误")}`,
        activeOlderVideoHistoryRequest: null,
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

  setSelectedVideoModel(modelId) {
    const state = get();
    if (state.isGeneratingVideo || !state.videoModels.some((model) => model.id === modelId)) return;
    set({ selectedVideoModelId: modelId, videoGenerationErrorMessage: null });
  },

  setVideoPrompt(videoPrompt) {
    if (!get().isGeneratingVideo) set({ videoPrompt, videoGenerationErrorMessage: null });
  },

  setVideoSize(videoSize) {
    if (!get().isGeneratingVideo && supportedVideoSizes.includes(videoSize)) {
      set({ videoSize, videoGenerationErrorMessage: null });
    }
  },

  setVideoSeconds(videoSeconds) {
    if (!get().isGeneratingVideo && Number.isInteger(videoSeconds)
      && videoSeconds >= 4 && videoSeconds <= 15) {
      set({ videoSeconds, videoGenerationErrorMessage: null });
    }
  },

  setVideoRatio(videoRatio) {
    if (!get().isGeneratingVideo && supportedVideoRatios.includes(videoRatio)) {
      set({ videoRatio, videoGenerationErrorMessage: null });
    }
  },

  async pickVideoInputImage(port) {
    const identity = beginVideoInputRequest(get, set, false);
    if (!identity) return;
    try {
      const result = await port.pickReferenceImages(referenceScope(identity));
      if (!acceptsVideoInput(get(), identity)) {
        discardLateReferences(port, identity, result.references);
        return;
      }
      const [selected, ...unused] = result.references;
      if (!selected) {
        set({
          isImportingVideoInput: false,
          activeVideoInputRequest: null,
          videoInputErrorMessage: result.errors[0] ?? null,
        });
        return;
      }
      const previous = get().videoInputImage;
      const discarded = [previous, ...unused].filter(
        (value): value is MediaStudioReference => value != null && value.id !== selected.id,
      );
      set({
        videoInputImage: selected,
        isImportingVideoInput: false,
        activeVideoInputRequest: null,
        videoInputErrorMessage: result.errors[0] ?? null,
      });
      discardLateReferences(port, identity, discarded);
    } catch (error) {
      failVideoInputRequest(error, identity, get, set, false);
    }
  },

  async addHistoryVideoInput(port, asset) {
    const state = get();
    if (state.isGeneratingVideo) return;
    if (!authorizedHistoryReferences(state.history).has(historyReferenceKey(asset))) {
      set({ videoInputErrorMessage: "只能使用当前账号图片历史中的资源。" });
      return;
    }
    const identity = beginVideoInputRequest(get, set, false);
    if (!identity) return;
    try {
      const result = await port.stageHistoryReferences({ ...referenceScope(identity), assets: [asset] });
      if (!acceptsVideoInput(get(), identity)) {
        discardLateReferences(port, identity, result.references);
        return;
      }
      const selected = result.references[0];
      if (!selected) {
        set({
          isImportingVideoInput: false,
          activeVideoInputRequest: null,
          videoInputErrorMessage: result.errors[0] ?? "参考图暂时无法读取。",
        });
        return;
      }
      const previous = get().videoInputImage;
      set({
        videoInputImage: selected,
        isImportingVideoInput: false,
        activeVideoInputRequest: null,
        videoInputErrorMessage: result.errors[0] ?? null,
      });
      if (previous && previous.id !== selected.id) discardLateReferences(port, identity, [previous]);
    } catch (error) {
      failVideoInputRequest(error, identity, get, set, false);
    }
  },

  async removeVideoInputImage(port) {
    const state = get();
    if (state.isGeneratingVideo || !state.videoInputImage) return;
    const identity = beginVideoInputRequest(get, set, false);
    if (!identity) return;
    const referenceId = state.videoInputImage.id;
    try {
      await port.discardReferenceImages({ ...referenceScope(identity), referenceIds: [referenceId] });
      if (!acceptsVideoInput(get(), identity)) return;
      set({
        videoInputImage: get().videoInputImage?.id === referenceId ? null : get().videoInputImage,
        isImportingVideoInput: false,
        activeVideoInputRequest: null,
        videoInputErrorMessage: null,
      });
    } catch (error) {
      failVideoInputRequest(error, identity, get, set, false);
    }
  },

  async pickVideoReferenceAudio(port) {
    const identity = beginVideoInputRequest(get, set, true);
    if (!identity) return;
    try {
      const audio = await port.pickVideoReferenceAudio(referenceScope(identity));
      if (!acceptsVideoAudio(get(), identity)) {
        if (audio) discardLateAudio(port, identity, audio);
        return;
      }
      set({
        videoReferenceAudio: audio ?? get().videoReferenceAudio,
        isImportingVideoAudio: false,
        activeVideoAudioRequest: null,
        videoInputErrorMessage: null,
      });
    } catch (error) {
      failVideoInputRequest(error, identity, get, set, true);
    }
  },

  async removeVideoReferenceAudio(port) {
    const state = get();
    if (state.isGeneratingVideo || !state.videoReferenceAudio) return;
    const identity = beginVideoInputRequest(get, set, true);
    if (!identity) return;
    const referenceAudioId = state.videoReferenceAudio.id;
    try {
      await port.discardVideoReferenceAudio({ ...referenceScope(identity), referenceAudioId });
      if (!acceptsVideoAudio(get(), identity)) return;
      set({
        videoReferenceAudio: get().videoReferenceAudio?.id === referenceAudioId
          ? null
          : get().videoReferenceAudio,
        isImportingVideoAudio: false,
        activeVideoAudioRequest: null,
        videoInputErrorMessage: null,
      });
    } catch (error) {
      failVideoInputRequest(error, identity, get, set, true);
    }
  },

  async generateVideo(port) {
    const identity = beginVideoGeneration(get, set);
    if (!identity) return;
    const state = get();
    const clientRequestId = newUuid();
    const request = {
      presentationId: identity.presentationId,
      modelConfigId: state.selectedVideoModelId!,
      prompt: state.videoPrompt.trim(),
      size: state.videoSize,
      seconds: state.videoSeconds,
      ratio: state.videoRatio,
      inputImageId: state.videoInputImage?.id ?? null,
      referenceAudioId: state.videoReferenceAudio?.id ?? null,
      clientRequestId,
    };
    set({ activeVideoClientRequestId: clientRequestId });
    try {
      const snapshot = await port.startVideoGeneration({ ownerUserId: identity.ownerUserId, request });
      if (!acceptsVideoGeneration(get(), identity, clientRequestId)) {
        void port.cancelVideoGeneration({
          ...referenceScope(identity),
          jobId: snapshot.jobId,
        }).catch(() => undefined);
        return;
      }
      applyVideoSnapshot(port, snapshot, identity, get, set);
    } catch (error) {
      if (!acceptsVideoGeneration(get(), identity, clientRequestId)) return;
      set({
        isGeneratingVideo: false,
        activeVideoGenerationRequest: null,
        activeVideoClientRequestId: null,
        videoGenerationErrorMessage: errorMessage(error, "视频任务创建失败。"),
        videoJob: failedVideoSnapshot(clientRequestId, error),
      });
    }
  },

  async pollVideo(port) {
    clearVideoPollTimer();
    const state = get();
    if (!state.isGeneratingVideo || !state.videoJob || state.videoJob.terminal) return;
    const identity = operationIdentity(state);
    if (!identity || state.activeVideoGenerationRequest == null) return;
    const generationRequestId = state.activeVideoGenerationRequest;
    const jobId = state.videoJob.jobId;
    const clientRequestId = state.videoJob.clientRequestId;
    set({ requestSequence: identity.requestId, activeVideoPollRequest: identity.requestId });
    try {
      const snapshot = await port.pollVideoGeneration({
        ...referenceScope(identity),
        jobId,
        clientRequestId,
      });
      if (!acceptsVideoPoll(
        get(), identity, generationRequestId, jobId, clientRequestId,
      )) return;
      applyVideoSnapshot(port, snapshot, {
        ...identity,
        requestId: generationRequestId,
      }, get, set);
    } catch (error) {
      if (!acceptsVideoPoll(
        get(), identity, generationRequestId, jobId, clientRequestId,
      )) return;
      set({
        isGeneratingVideo: false,
        activeVideoGenerationRequest: null,
        activeVideoPollRequest: null,
        activeVideoClientRequestId: null,
        videoGenerationErrorMessage: errorMessage(error, "视频状态查询失败。"),
        videoJob: {
          ...state.videoJob,
          status: "failed",
          errorMessage: errorMessage(error, "视频状态查询失败。"),
          terminal: true,
          revision: state.videoJob.revision + 1,
        },
      });
    }
  },

  async cancelVideo(port) {
    const state = get();
    if (!state.isGeneratingVideo) return;
    clearVideoPollTimer();
    set({
      isGeneratingVideo: false,
      videoJob: null,
      activeVideoGenerationRequest: null,
      activeVideoPollRequest: null,
      activeVideoClientRequestId: null,
      videoGenerationErrorMessage: null,
      requestSequence: state.requestSequence + 1,
    });
    if (!state.ownerUserId || !state.presentationId || !state.videoJob) return;
    try {
      await port.cancelVideoGeneration({
        ownerUserId: state.ownerUserId,
        presentationId: state.presentationId,
        jobId: state.videoJob.jobId,
      });
    } catch (error) {
      const latest = get();
      if (latest.ownerUserId === state.ownerUserId
        && latest.presentationId === state.presentationId
        && !latest.isGeneratingVideo) {
        set({ videoGenerationErrorMessage: errorMessage(error, "未能取消视频任务。") });
      }
    }
  },

  dismissVideoError() { set({ videoGenerationErrorMessage: null }); },
  dismissVideoHistoryFeedback() { set({ videoHistoryErrorMessage: null }); },

  deactivate(port) {
    ++activationSequence;
    clearVideoPollTimer();
    const state = get();
    queuePresentationDiscard(port, state);
    set(resetState(state));
  },

  reset(port) {
    ++activationSequence;
    clearVideoPollTimer();
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

export function canGenerateMediaStudioVideo(state: MediaStudioState) {
  return state.ownerUserId != null
    && state.presentationId != null
    && !state.isGeneratingVideo
    && !state.isImportingVideoInput
    && !state.isImportingVideoAudio
    && state.selectedVideoModelId != null
    && state.videoModels.some((model) => model.id === state.selectedVideoModelId)
    && state.videoPrompt.trim().length > 0;
}

export type MediaStudioVideoCanvasState =
  | { kind: "empty" }
  | { kind: "progress"; status: "submitting" | MediaStudioVideoJobStatus; percent: number | null }
  | { kind: "failed"; message: string }
  | { kind: "result"; item: MediaStudioVideoHistoryItem };

export function mediaStudioVideoCanvasState(state: MediaStudioState): MediaStudioVideoCanvasState {
  if (state.isGeneratingVideo) {
    return {
      kind: "progress",
      status: state.videoJob?.status ?? "submitting",
      percent: state.videoJob?.percent ?? null,
    };
  }
  if (state.videoJob?.status === "failed") {
    return {
      kind: "failed",
      message: state.videoJob.errorMessage ?? state.videoGenerationErrorMessage ?? "视频生成失败。",
    };
  }
  const item = state.videoJob?.result ?? state.videoHistory[0];
  return item ? { kind: "result", item } : { kind: "empty" };
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
export const supportedVideoSizes: MediaStudioVideoSize[] = ["768P", "2K"];
export const supportedVideoRatios: MediaStudioVideoRatio[] = [
  "16:9", "9:16", "1:1", "4:3", "3:4", "21:9",
];

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
    videoModels: [] as MediaStudioModel[],
    selectedVideoModelId: null,
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
    videoHistory: [] as MediaStudioVideoHistoryItem[],
    videoHistoryCursor: null,
    unreadableVideoHistoryCount: 0,
    isLoadingVideoHistory: false,
    hasLoadedVideoHistory: false,
    isLoadingOlderVideoHistory: false,
    videoHistoryErrorMessage: null,
    activeVideoHistoryRequest: null,
    activeOlderVideoHistoryRequest: null,
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
    videoPrompt: "",
    videoSize: "768P" as MediaStudioVideoSize,
    videoSeconds: 6,
    videoRatio: "16:9" as MediaStudioVideoRatio,
    videoInputImage: null,
    videoReferenceAudio: null,
    isImportingVideoInput: false,
    isImportingVideoAudio: false,
    videoInputErrorMessage: null,
    activeVideoInputRequest: null,
    activeVideoAudioRequest: null,
    isGeneratingVideo: false,
    videoJob: null,
    videoGenerationErrorMessage: null,
    activeVideoGenerationRequest: null,
    activeVideoPollRequest: null,
    activeVideoClientRequestId: null,
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

function beginVideoHistoryRequest(
  get: StoreGet,
  set: StoreSet,
  older: boolean,
): OperationIdentity | null {
  const state = get();
  if (state.isLoadingOlderVideoHistory || (older && state.isLoadingVideoHistory)) return null;
  if (older && !state.videoHistoryCursor) return null;
  const identity = operationIdentity(state);
  if (!identity) return null;
  if (older) {
    set({
      requestSequence: identity.requestId,
      activeOlderVideoHistoryRequest: identity.requestId,
      isLoadingOlderVideoHistory: true,
      videoHistoryErrorMessage: unreadableVideoHistoryMessage(state.unreadableVideoHistoryCount),
    });
  } else {
    set({
      requestSequence: identity.requestId,
      activeVideoHistoryRequest: identity.requestId,
      isLoadingVideoHistory: true,
      videoHistoryErrorMessage: null,
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

function beginVideoInputRequest(
  get: StoreGet,
  set: StoreSet,
  audio: boolean,
): OperationIdentity | null {
  const state = get();
  if (state.isGeneratingVideo || state.isImportingVideoInput || state.isImportingVideoAudio) {
    return null;
  }
  const identity = operationIdentity(state);
  if (!identity) return null;
  set(audio ? {
    requestSequence: identity.requestId,
    activeVideoAudioRequest: identity.requestId,
    isImportingVideoAudio: true,
    videoInputErrorMessage: null,
  } : {
    requestSequence: identity.requestId,
    activeVideoInputRequest: identity.requestId,
    isImportingVideoInput: true,
    videoInputErrorMessage: null,
  });
  return identity;
}

function beginVideoGeneration(get: StoreGet, set: StoreSet): OperationIdentity | null {
  const state = get();
  if (!canGenerateMediaStudioVideo(state)) return null;
  const identity = operationIdentity(state);
  if (!identity) return null;
  clearVideoPollTimer();
  set({
    requestSequence: identity.requestId,
    activeVideoGenerationRequest: identity.requestId,
    activeVideoPollRequest: null,
    activeVideoClientRequestId: null,
    isGeneratingVideo: true,
    videoJob: null,
    videoGenerationErrorMessage: null,
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

function acceptsInitialVideoHistory(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeVideoHistoryRequest === identity.requestId;
}

function acceptsOlderVideoHistory(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeOlderVideoHistoryRequest === identity.requestId;
}

function acceptsReferences(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeReferenceRequest === identity.requestId;
}

function acceptsGeneration(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeGenerationRequest === identity.requestId;
}

function acceptsVideoInput(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeVideoInputRequest === identity.requestId;
}

function acceptsVideoAudio(state: MediaStudioState, identity: OperationIdentity) {
  return acceptsBase(state, identity) && state.activeVideoAudioRequest === identity.requestId;
}

function acceptsVideoGeneration(
  state: MediaStudioState,
  identity: OperationIdentity,
  clientRequestId: string,
) {
  return acceptsBase(state, identity)
    && state.activeVideoGenerationRequest === identity.requestId
    && state.activeVideoClientRequestId === clientRequestId;
}

function acceptsVideoPoll(
  state: MediaStudioState,
  identity: OperationIdentity,
  generationRequestId: number,
  jobId: string,
  clientRequestId: string,
) {
  return acceptsBase(state, identity)
    && state.activeVideoGenerationRequest === generationRequestId
    && state.activeVideoPollRequest === identity.requestId
    && state.activeVideoClientRequestId === clientRequestId
    && state.videoJob?.jobId === jobId
    && state.videoJob.clientRequestId === clientRequestId;
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

function failVideoInputRequest(
  error: unknown,
  identity: OperationIdentity,
  get: StoreGet,
  set: StoreSet,
  audio: boolean,
) {
  const accepts = audio ? acceptsVideoAudio(get(), identity) : acceptsVideoInput(get(), identity);
  if (!accepts) return;
  set(audio ? {
    isImportingVideoAudio: false,
    activeVideoAudioRequest: null,
    videoInputErrorMessage: errorMessage(error, "参考音频暂时无法读取。"),
  } : {
    isImportingVideoInput: false,
    activeVideoInputRequest: null,
    videoInputErrorMessage: errorMessage(error, "视频首帧暂时无法读取。"),
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

function discardLateAudio(
  port: MediaStudioPort,
  identity: OperationIdentity,
  audio: MediaStudioVideoAudioDraft,
) {
  void port.discardVideoReferenceAudio({
    ...referenceScope(identity),
    referenceAudioId: audio.id,
  }).catch(() => undefined);
}

function applyVideoSnapshot(
  port: MediaStudioPort,
  snapshot: MediaStudioVideoJobSnapshot,
  generationIdentity: OperationIdentity,
  get: StoreGet,
  set: StoreSet,
) {
  const state = get();
  if (snapshot.clientRequestId !== state.activeVideoClientRequestId
    || (state.videoJob != null && snapshot.jobId !== state.videoJob.jobId)
    || (state.videoJob != null && snapshot.revision < state.videoJob.revision)) {
    set({
      isGeneratingVideo: false,
      activeVideoGenerationRequest: null,
      activeVideoPollRequest: null,
      activeVideoClientRequestId: null,
      videoGenerationErrorMessage: "视频任务返回了不匹配的状态。",
      videoJob: failedVideoSnapshot(snapshot.clientRequestId, "视频任务返回了不匹配的状态。"),
    });
    return;
  }
  if (snapshot.status === "cancelled") {
    set({
      isGeneratingVideo: false,
      videoJob: null,
      activeVideoGenerationRequest: null,
      activeVideoPollRequest: null,
      activeVideoClientRequestId: null,
      videoGenerationErrorMessage: null,
    });
    return;
  }
  if (snapshot.status === "failed") {
    set({
      isGeneratingVideo: false,
      videoJob: snapshot,
      activeVideoGenerationRequest: null,
      activeVideoPollRequest: null,
      activeVideoClientRequestId: null,
      videoGenerationErrorMessage: snapshot.errorMessage ?? "视频生成失败。",
    });
    return;
  }
  if (snapshot.terminal) {
    if (snapshot.status === "completed" && snapshot.result) {
      set({
        isGeneratingVideo: false,
        videoJob: snapshot,
        videoHistory: mergeVideoHistory([snapshot.result], state.videoHistory),
        activeVideoGenerationRequest: null,
        activeVideoPollRequest: null,
        activeVideoClientRequestId: null,
        videoGenerationErrorMessage: null,
      });
    } else {
      const message = snapshot.errorMessage ?? "视频任务结束，但没有返回可播放的本机结果。";
      set({
        isGeneratingVideo: false,
        videoJob: { ...snapshot, status: "failed", errorMessage: message, terminal: true },
        activeVideoGenerationRequest: null,
        activeVideoPollRequest: null,
        activeVideoClientRequestId: null,
        videoGenerationErrorMessage: message,
      });
    }
    return;
  }
  set({
    isGeneratingVideo: true,
    videoJob: snapshot,
    activeVideoPollRequest: null,
    videoGenerationErrorMessage: null,
  });
  scheduleVideoPoll(port, generationIdentity);
}

function scheduleVideoPoll(port: MediaStudioPort, identity: OperationIdentity) {
  clearVideoPollTimer();
  videoPollTimer = setTimeout(() => {
    const state = useMediaStudioStore.getState();
    if (!acceptsBase(state, identity)
      || state.activeVideoGenerationRequest !== identity.requestId
      || !state.isGeneratingVideo) return;
    void state.pollVideo(port);
  }, mediaStudioVideoPollIntervalMs);
}

function clearVideoPollTimer() {
  if (videoPollTimer != null) clearTimeout(videoPollTimer);
  videoPollTimer = null;
}

function failedVideoSnapshot(
  clientRequestId: string,
  error: unknown,
): MediaStudioVideoJobSnapshot {
  return {
    jobId: newUuid(),
    clientRequestId,
    status: "failed",
    percent: null,
    providerJobId: null,
    result: null,
    errorMessage: errorMessage(error, "视频生成失败。"),
    terminal: true,
    revision: 1,
  };
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

function mergeVideoHistory(
  current: MediaStudioVideoHistoryItem[],
  incoming: MediaStudioVideoHistoryItem[],
) {
  const values = new Map<string, MediaStudioVideoHistoryItem>();
  for (const item of [...current, ...incoming]) {
    if (!values.has(item.id)) values.set(item.id, item);
  }
  return sortedVideoHistory([...values.values()]);
}

function sortedVideoHistory(items: MediaStudioVideoHistoryItem[]) {
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

function unreadableVideoHistoryMessage(count: number) {
  return count > 0 ? `有 ${count} 条视频记录无法读取，原文件已保留。` : null;
}

function isSupportedVideoModel(model: MediaStudioModel) {
  return model.modelName.toLocaleLowerCase("en-US") === "minimax-h3";
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

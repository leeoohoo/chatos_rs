import { create, type StoreApi } from "zustand";
import type { MediaStudioHistoryItem, MediaStudioModel } from "../../media-studio/domain/mediaStudio";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import type { StoryMediaBatchEvent, StoryMediaBatchPreparation, StoryMediaBatchSnapshot, StoryPlanningEvent, StoryPlanningRun, StoryProject, StoryProjectCursor, StoryVideoGenerationEvent } from "../domain/storyStudio";
import type { StoryImageImportTarget, StoryStudioPort } from "../ports/StoryStudioPort";

interface StoryVideoProgress {
  status: string;
  percent: number | null;
}

export interface StoryPlanningProgress {
  runId: string | null;
  projectId: string;
  stage: StoryPlanningEvent["run"]["stage"];
  status: StoryPlanningEvent["run"]["status"];
  stopReason: string | null;
  modelCalls: number;
  updatedAtUnixMs: number;
}

interface StoryStudioState {
  ownerUserId: string | null;
  presentationId: string | null;
  projects: StoryProject[];
  selectedProjectId: string | null;
  models: MediaStudioModel[];
  storyImageHistory: MediaStudioHistoryItem[];
  storyImageHistoryLoading: boolean;
  loading: boolean;
  busy: boolean;
  error: string | null;
  unreadableCount: number;
  planningProgress: Record<string, StoryPlanningProgress>;
  planningRuns: Record<string, StoryPlanningRun[]>;
  planningHistoryUnreadable: Record<string, number>;
  planningHistoryNextCursor: Record<string, StoryProjectCursor | null>;
  mediaBatches: Record<string, StoryMediaBatchSnapshot[]>;
  mediaBatchHistoryUnreadable: Record<string, number>;
  mediaBatchHistoryNextCursor: Record<string, StoryProjectCursor | null>;
  videoProgress: Record<string, StoryVideoProgress>;
  activeMediaTarget: string | null;
  activate(storyPort: StoryStudioPort, mediaPort: MediaStudioPort, ownerUserId: string): Promise<void>;
  deactivate(storyPort: StoryStudioPort): void;
  selectProject(storyPort: StoryStudioPort, projectId: string): Promise<void>;
  createProject(storyPort: StoryStudioPort, input: {
    title: string;
    description: string;
    textModelId: string;
    imageModelId: string;
    videoModelId: string;
  }): Promise<void>;
  updateProjectSettings(storyPort: StoryStudioPort, input: {
    title: string;
    description: string;
    textModelId: string;
    imageModelId: string;
    videoModelId: string;
  }): Promise<boolean>;
  saveSource(storyPort: StoryStudioPort, input: {
    source: string;
    style: string;
    ratio: string;
  }): Promise<void>;
  updateResourcePrompt(storyPort: StoryStudioPort, resourceId: string, prompt: string): Promise<boolean>;
  startPlanning(storyPort: StoryStudioPort, stage: "outline" | "refine", targetIds: string[], userIdeas: string): Promise<void>;
  loadPlanningHistory(storyPort: StoryStudioPort, projectId: string): Promise<void>;
  loadMorePlanningHistory(storyPort: StoryStudioPort, projectId: string): Promise<void>;
  resumePlanning(storyPort: StoryStudioPort, projectId: string, runId: string): Promise<void>;
  abandonPlanning(storyPort: StoryStudioPort, projectId: string, runId: string): Promise<void>;
  loadMediaBatchHistory(storyPort: StoryStudioPort, projectId: string): Promise<void>;
  loadMoreMediaBatchHistory(storyPort: StoryStudioPort, projectId: string): Promise<void>;
  startMediaBatch(storyPort: StoryStudioPort, preparation: StoryMediaBatchPreparation, userIdeas: string): Promise<boolean>;
  resumeMediaBatch(storyPort: StoryStudioPort, projectId: string, batchId: string): Promise<void>;
  abandonMediaBatch(storyPort: StoryStudioPort, projectId: string, batchId: string): Promise<void>;
  importLocalImage(storyPort: StoryStudioPort, target: StoryImageImportTarget): Promise<void>;
  loadStoryImageHistory(mediaPort: MediaStudioPort): Promise<void>;
  importHistoryImage(storyPort: StoryStudioPort, target: StoryImageImportTarget, recordId: string, contentToken: string): Promise<void>;
  updateSegmentDuration(storyPort: StoryStudioPort, segmentId: string, seconds: number): Promise<void>;
  updateSegmentStructure(storyPort: StoryStudioPort, input: {
    operation: "add" | "delete" | "move";
    segmentId?: string | null;
    offset?: -1 | 1 | null;
  }): Promise<StoryProject | null>;
  allowImageRetry(storyPort: StoryStudioPort, target: StoryImageImportTarget): Promise<void>;
  generateAssetImage(storyPort: StoryStudioPort, resourceId: string, userIdeas: string): Promise<void>;
  generateFrameImage(storyPort: StoryStudioPort, segmentId: string, frameRole: "first" | "last", referenceResourceIds: string[], userIdeas: string): Promise<void>;
  confirmLatestResources(storyPort: StoryStudioPort, resourceIds: string[]): Promise<void>;
  confirmLatestFrames(storyPort: StoryStudioPort, segmentId: string, useLastFrame: boolean): Promise<void>;
  startVideo(storyPort: StoryStudioPort, segmentId: string, size: "768P" | "2K", userIdeas: string): Promise<void>;
  resumeVideo(storyPort: StoryStudioPort, segmentId: string): Promise<void>;
  updateVideoGuidance(storyPort: StoryStudioPort, segmentId: string, mode: "first_frame" | "first_and_last_frames" | "previous_video" | "source_video"): Promise<void>;
  archiveCompletedVideo(storyPort: StoryStudioPort, segmentId: string, useSourceVideo: boolean): Promise<void>;
  allowVideoRetry(storyPort: StoryStudioPort, segmentId: string): Promise<void>;
  clearError(): void;
}

let activationSequence = 0;
let planningWatchSequence = 0;
let planningUnlisten: (() => void) | null = null;
let mediaBatchWatchSequence = 0;
let mediaBatchUnlisten: (() => void) | null = null;
let videoGenerationWatchSequence = 0;
let videoGenerationUnlisten: (() => void) | null = null;
const pollTimers = new Map<string, number>();

export const useStoryStudioStore = create<StoryStudioState>((set, get) => ({
  ownerUserId: null,
  presentationId: null,
  projects: [],
  selectedProjectId: null,
  models: [],
  storyImageHistory: [],
  storyImageHistoryLoading: false,
  loading: false,
  busy: false,
  error: null,
  unreadableCount: 0,
  planningProgress: {},
  planningRuns: {},
  planningHistoryUnreadable: {},
  planningHistoryNextCursor: {},
  mediaBatches: {},
  mediaBatchHistoryUnreadable: {},
  mediaBatchHistoryNextCursor: {},
  videoProgress: {},
  activeMediaTarget: null,

  async activate(storyPort, mediaPort, ownerUserId) {
    const sequence = ++activationSequence;
    stopPlanningWatch();
    stopMediaBatchWatch();
    stopVideoGenerationWatch();
    clearPollTimers();
    const presentationId = crypto.randomUUID();
    set({
      ownerUserId,
      presentationId,
      projects: [],
      selectedProjectId: null,
      models: [],
      storyImageHistory: [],
      storyImageHistoryLoading: false,
      loading: true,
      busy: false,
      error: null,
      unreadableCount: 0,
      planningProgress: {},
      planningRuns: {},
      planningHistoryUnreadable: {},
      planningHistoryNextCursor: {},
      mediaBatches: {},
      mediaBatchHistoryUnreadable: {},
      mediaBatchHistoryNextCursor: {},
      videoProgress: {},
      activeMediaTarget: null,
    });
    installPlanningWatch(storyPort, { ownerUserId, presentationId }, sequence, set, get);
    installMediaBatchWatch(storyPort, { ownerUserId, presentationId }, sequence, set, get);
    installVideoGenerationWatch(storyPort, { ownerUserId, presentationId }, sequence, set, get);
    try {
      const [page, models] = await Promise.all([
        storyPort.loadProjects({ ownerUserId, presentationId, limit: 80 }),
        mediaPort.listModels(ownerUserId),
      ]);
      if (!isActive(get(), sequence, ownerUserId, presentationId)) return;
      set({
        projects: page.items,
        selectedProjectId: page.items[0]?.id ?? null,
        models,
        loading: false,
        unreadableCount: page.unreadableCount,
      });
      if (page.items[0]) {
        void get().loadPlanningHistory(storyPort, page.items[0].id);
        void get().loadMediaBatchHistory(storyPort, page.items[0].id);
      }
    } catch (error) {
      if (!isActive(get(), sequence, ownerUserId, presentationId)) return;
      set({ loading: false, error: errorMessage(error, "剧情项目暂时无法读取。") });
    }
  },

  deactivate(storyPort) {
    activationSequence += 1;
    stopPlanningWatch();
    stopMediaBatchWatch();
    stopVideoGenerationWatch();
    clearPollTimers();
    const { ownerUserId, presentationId } = get();
    set({
      ownerUserId: null,
      presentationId: null,
      projects: [],
      selectedProjectId: null,
      models: [],
      storyImageHistory: [],
      storyImageHistoryLoading: false,
      loading: false,
      busy: false,
      error: null,
      unreadableCount: 0,
      planningProgress: {},
      planningRuns: {},
      planningHistoryUnreadable: {},
      planningHistoryNextCursor: {},
      mediaBatches: {},
      mediaBatchHistoryUnreadable: {},
      mediaBatchHistoryNextCursor: {},
      videoProgress: {},
      activeMediaTarget: null,
    });
    if (ownerUserId && presentationId) {
      void storyPort.discardPresentation({ ownerUserId, presentationId }).catch(() => undefined);
    }
  },

  async selectProject(storyPort, projectId) {
    const scope = activeScope(get());
    if (!scope) return;
    set({ selectedProjectId: projectId, loading: true, error: null });
    try {
      const project = await storyPort.loadProject({ ...scope, projectId });
      if (!sameScope(get(), scope) || get().selectedProjectId !== projectId) return;
      set((state) => ({ projects: replaceProject(state.projects, project), loading: false }));
      void get().loadPlanningHistory(storyPort, projectId);
      void get().loadMediaBatchHistory(storyPort, projectId);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ loading: false, error: errorMessage(error, "剧情项目暂时无法打开。") });
    }
  },

  async createProject(storyPort, input) {
    const scope = activeScope(get());
    if (!scope || get().busy) return;
    set({ busy: true, error: null });
    try {
      const project = await storyPort.createProject({ ...scope, ...input });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: [project, ...state.projects.filter((value) => value.id !== project.id)],
        selectedProjectId: project.id,
        busy: false,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情项目创建失败。") });
    }
  },

  async updateProjectSettings(storyPort, input) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project || get().busy) return false;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateProjectSettings({
        ...scope,
        projectId: project.id,
        ...input,
      });
      if (!sameScope(get(), scope)) return false;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
      return true;
    } catch (error) {
      if (!sameScope(get(), scope)) return false;
      set({ busy: false, error: errorMessage(error, "剧情设置保存失败。") });
      return false;
    }
  },

  async saveSource(storyPort, input) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateSource({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        ...input,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情原文保存失败。") });
    }
  },

  async updateResourcePrompt(storyPort, resourceId, prompt) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return false;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateResourcePrompt({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        resourceId,
        prompt,
      });
      if (!sameScope(get(), scope)) return false;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
      return true;
    } catch (error) {
      if (!sameScope(get(), scope)) return false;
      set({ busy: false, error: errorMessage(error, "素材描述保存失败。") });
      return false;
    }
  },

  async startPlanning(storyPort, stage, targetIds, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project || get().busy) return;
    set((state) => ({
      busy: true,
      error: null,
      planningProgress: {
        ...state.planningProgress,
        [project.id]: {
          runId: null,
          projectId: project.id,
          stage,
          status: "ready",
          stopReason: null,
          modelCalls: 0,
          updatedAtUnixMs: Date.now(),
        },
      },
    }));
    try {
      const planned = await storyPort.startPlanning({
        ...scope,
        projectId: project.id,
        stage,
        targetIds,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: planned ? replaceProject(state.projects, planned) : state.projects,
        busy: false,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情规划没有完成。") });
    } finally {
      if (sameScope(get(), scope)) void get().loadPlanningHistory(storyPort, project.id);
    }
  },

  async loadPlanningHistory(storyPort, projectId) {
    const scope = activeScope(get());
    if (!scope) return;
    try {
      const page = await storyPort.loadPlanningHistory({ ...scope, projectId, limit: 20 });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: page.recoveredProject
          ? replaceProject(state.projects, page.recoveredProject)
          : state.projects,
        planningRuns: { ...state.planningRuns, [projectId]: page.runs },
        planningHistoryUnreadable: {
          ...state.planningHistoryUnreadable,
          [projectId]: page.unreadableCount,
        },
        planningHistoryNextCursor: {
          ...state.planningHistoryNextCursor,
          [projectId]: page.nextCursor,
        },
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ error: errorMessage(error, "剧情规划历史暂时无法读取。") });
    }
  },

  async loadMorePlanningHistory(storyPort, projectId) {
    const scope = activeScope(get());
    const cursor = get().planningHistoryNextCursor[projectId];
    if (!scope || !cursor) return;
    try {
      const page = await storyPort.loadPlanningHistory({
        ...scope,
        projectId,
        after: cursor,
        limit: 20,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: page.recoveredProject
          ? replaceProject(state.projects, page.recoveredProject)
          : state.projects,
        planningRuns: {
          ...state.planningRuns,
          [projectId]: mergePlanningRuns(state.planningRuns[projectId] ?? [], page.runs),
        },
        planningHistoryUnreadable: {
          ...state.planningHistoryUnreadable,
          [projectId]: (state.planningHistoryUnreadable[projectId] ?? 0) + page.unreadableCount,
        },
        planningHistoryNextCursor: {
          ...state.planningHistoryNextCursor,
          [projectId]: page.nextCursor,
        },
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ error: errorMessage(error, "更早的剧情规划记录暂时无法读取。") });
    }
  },

  async resumePlanning(storyPort, projectId, runId) {
    const scope = activeScope(get());
    const run = get().planningRuns[projectId]?.find((value) => value.id === runId);
    if (!scope || !run || get().busy) return;
    set((state) => ({
      busy: true,
      error: null,
      planningProgress: {
        ...state.planningProgress,
        [projectId]: planningProgressFromRun(run),
      },
    }));
    try {
      const project = await storyPort.resumePlanning({ ...scope, projectId, runId });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: project ? replaceProject(state.projects, project) : state.projects,
        busy: false,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情规划恢复失败。") });
    } finally {
      if (sameScope(get(), scope)) void get().loadPlanningHistory(storyPort, projectId);
    }
  },

  async abandonPlanning(storyPort, projectId, runId) {
    const scope = activeScope(get());
    if (!scope || get().busy) return;
    set({ busy: true, error: null });
    try {
      const run = await storyPort.abandonPlanning({ ...scope, projectId, runId });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        busy: false,
        planningRuns: {
          ...state.planningRuns,
          [projectId]: replacePlanningRun(state.planningRuns[projectId] ?? [], run),
        },
        planningProgress: withoutKey(state.planningProgress, projectId),
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情规划草稿无法放弃。") });
    }
  },

  async loadMediaBatchHistory(storyPort, projectId) {
    const scope = activeScope(get());
    if (!scope) return;
    try {
      const page = await storyPort.loadMediaBatchHistory({ ...scope, projectId, limit: 20 });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        mediaBatches: { ...state.mediaBatches, [projectId]: page.batches },
        mediaBatchHistoryUnreadable: {
          ...state.mediaBatchHistoryUnreadable,
          [projectId]: page.unreadableCount,
        },
        mediaBatchHistoryNextCursor: {
          ...state.mediaBatchHistoryNextCursor,
          [projectId]: page.nextCursor,
        },
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ error: errorMessage(error, "批量制作历史暂时无法读取。") });
    }
  },

  async loadMoreMediaBatchHistory(storyPort, projectId) {
    const scope = activeScope(get());
    const cursor = get().mediaBatchHistoryNextCursor[projectId];
    if (!scope || !cursor) return;
    try {
      const page = await storyPort.loadMediaBatchHistory({
        ...scope,
        projectId,
        after: cursor,
        limit: 20,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        mediaBatches: {
          ...state.mediaBatches,
          [projectId]: mergeMediaBatches(state.mediaBatches[projectId] ?? [], page.batches),
        },
        mediaBatchHistoryUnreadable: {
          ...state.mediaBatchHistoryUnreadable,
          [projectId]: (state.mediaBatchHistoryUnreadable[projectId] ?? 0) + page.unreadableCount,
        },
        mediaBatchHistoryNextCursor: {
          ...state.mediaBatchHistoryNextCursor,
          [projectId]: page.nextCursor,
        },
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ error: errorMessage(error, "更早的批量制作记录暂时无法读取。") });
    }
  },

  async startMediaBatch(storyPort, preparation, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project || project.id !== preparation.projectId || get().busy) return false;
    set({ busy: true, error: null });
    try {
      if (preparation.executionMode === "directVideos") {
        void storyPort.startDirectVideoQueue({ ...scope, preparation, userIdeas }).then((outcome) => {
          if (!sameScope(get(), scope)) return;
          set((state) => ({
            busy: false,
            projects: replaceProject(state.projects, outcome.project),
          }));
        }).catch((error: unknown) => {
          if (!sameScope(get(), scope)) return;
          set({ busy: false, error: errorMessage(error, "顺序视频队列已停止。") });
        });
        return true;
      }
      const batch = await storyPort.startMediaBatch({ ...scope, preparation, userIdeas });
      if (!sameScope(get(), scope)) return false;
      set((state) => ({
        mediaBatches: {
          ...state.mediaBatches,
          [project.id]: replaceMediaBatch(state.mediaBatches[project.id] ?? [], batch),
        },
      }));
      const outcome = await storyPort.resumeMediaBatch({
        ...scope,
        projectId: project.id,
        batchId: batch.id,
      });
      if (!sameScope(get(), scope)) return false;
      set((state) => ({
        busy: false,
        projects: replaceProject(state.projects, outcome.project),
        mediaBatches: {
          ...state.mediaBatches,
          [project.id]: replaceMediaBatch(state.mediaBatches[project.id] ?? [], outcome.batch),
        },
      }));
      return true;
    } catch (error) {
      if (!sameScope(get(), scope)) return false;
      set({ busy: false, error: errorMessage(error, "批量制作队列创建失败。") });
      return false;
    }
  },

  async resumeMediaBatch(storyPort, projectId, batchId) {
    const scope = activeScope(get());
    if (!scope || get().busy) return;
    set({ busy: true, error: null });
    try {
      const outcome = await storyPort.resumeMediaBatch({ ...scope, projectId, batchId });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        busy: false,
        projects: replaceProject(state.projects, outcome.project),
        mediaBatches: {
          ...state.mediaBatches,
          [projectId]: replaceMediaBatch(state.mediaBatches[projectId] ?? [], outcome.batch),
        },
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "制作批次无法恢复。") });
    }
  },

  async abandonMediaBatch(storyPort, projectId, batchId) {
    const scope = activeScope(get());
    if (!scope || get().busy) return;
    set({ busy: true, error: null });
    try {
      const batch = await storyPort.abandonMediaBatch({ ...scope, projectId, batchId });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        busy: false,
        mediaBatches: {
          ...state.mediaBatches,
          [projectId]: replaceMediaBatch(state.mediaBatches[projectId] ?? [], batch),
        },
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "制作批次无法结束。") });
    }
  },

  async importLocalImage(storyPort, target) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    const operation = target.kind === "resource"
      ? `import:resource:${target.resourceId}`
      : `import:frame:${target.segmentId}:${target.frameRole}`;
    set({ activeMediaTarget: operation, error: null });
    try {
      const saved = await storyPort.importLocalImage({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        target,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: saved ? replaceProject(state.projects, saved) : state.projects,
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "本机剧情图片导入失败。") });
    }
  },

  async loadStoryImageHistory(mediaPort) {
    const scope = activeScope(get());
    if (!scope || get().storyImageHistoryLoading) return;
    set({ storyImageHistoryLoading: true, error: null });
    try {
      const page = await mediaPort.loadImageHistory({ ownerUserId: scope.ownerUserId, limit: 40 });
      if (!sameScope(get(), scope)) return;
      set({ storyImageHistory: page.items, storyImageHistoryLoading: false });
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({
        storyImageHistoryLoading: false,
        error: errorMessage(error, "AI 创作图片历史暂时无法读取。"),
      });
    }
  },

  async importHistoryImage(storyPort, target, recordId, contentToken) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    const operation = target.kind === "resource"
      ? `history:resource:${target.resourceId}`
      : `history:frame:${target.segmentId}:${target.frameRole}`;
    set({ activeMediaTarget: operation, error: null });
    try {
      const saved = await storyPort.importHistoryImage({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        target,
        recordId,
        contentToken,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: replaceProject(state.projects, saved),
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "创作历史图片导入失败。") });
    }
  },

  async updateSegmentDuration(storyPort, segmentId, seconds) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateSegmentDuration({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        seconds,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "分段时长保存失败。") });
    }
  },

  async updateSegmentStructure(storyPort, input) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return null;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateSegmentStructure({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        ...input,
      });
      if (!sameScope(get(), scope)) return null;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
      return saved;
    } catch (error) {
      if (!sameScope(get(), scope)) return null;
      set({ busy: false, error: errorMessage(error, "剧情结构保存失败。") });
      return null;
    }
  },

  async allowImageRetry(storyPort, target) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: "image-retry", error: null });
    try {
      const saved = await storyPort.allowImageRetry({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        target,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: replaceProject(state.projects, saved),
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "图片任务无法解锁重试。") });
    }
  },

  async generateAssetImage(storyPort, resourceId, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: `resource:${resourceId}`, error: null });
    try {
      const saved = await storyPort.generateAssetImage({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        resourceId,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: replaceProject(state.projects, saved),
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "剧情素材图片生成失败。") });
    }
  },

  async generateFrameImage(storyPort, segmentId, frameRole, referenceResourceIds, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: `frame:${segmentId}:${frameRole}`, error: null });
    try {
      const saved = await storyPort.generateFrameImage({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        frameRole,
        referenceResourceIds,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: replaceProject(state.projects, saved),
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "剧情首尾帧生成失败。") });
    }
  },

  async confirmLatestResources(storyPort, resourceIds) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: "confirm:resources", error: null });
    try {
      const saved = await storyPort.confirmLatestResourceImages({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        resourceIds,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), activeMediaTarget: null }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "素材版本确认失败。") });
    }
  },

  async confirmLatestFrames(storyPort, segmentId, useLastFrame) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: `confirm:frames:${segmentId}`, error: null });
    try {
      const saved = await storyPort.confirmLatestFrames({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        useConfirmedLastFrameForVideo: useLastFrame,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), activeMediaTarget: null }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "首尾帧版本确认失败。") });
    }
  },

  async startVideo(storyPort, segmentId, size, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set((state) => ({
      error: null,
      videoProgress: { ...state.videoProgress, [segmentId]: { status: "submitting", percent: null } },
    }));
    try {
      const outcome = await storyPort.startVideo({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        size,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      acceptVideoOutcome(set, get, storyPort, scope, segmentId, outcome);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        error: errorMessage(error, "剧情视频提交失败。"),
        videoProgress: withoutKey(state.videoProgress, segmentId),
      }));
    }
  },

  async resumeVideo(storyPort, segmentId) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project) return;
    set((state) => ({
      error: null,
      videoProgress: { ...state.videoProgress, [segmentId]: { status: "checking", percent: null } },
    }));
    try {
      const outcome = await storyPort.pollVideo({
        ...scope,
        projectId: project.id,
        segmentId,
      });
      if (!sameScope(get(), scope)) return;
      acceptVideoOutcome(set, get, storyPort, scope, segmentId, outcome);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        error: errorMessage(error, "剧情视频任务无法恢复。"),
        videoProgress: withoutKey(state.videoProgress, segmentId),
      }));
    }
  },

  async updateVideoGuidance(storyPort, segmentId, mode) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateVideoGuidance({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        mode,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "视频引导方式保存失败。") });
    }
  },

  async archiveCompletedVideo(storyPort, segmentId, useSourceVideo) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.archiveCompletedVideo({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        useSourceVideo,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "完成视频归档失败。") });
    }
  },

  async allowVideoRetry(storyPort, segmentId) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.allowVideoRetry({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "视频任务无法解锁重试。") });
    }
  },

  clearError() {
    set({ error: null });
  },
}));

function acceptVideoOutcome(
  set: StoreApi<StoryStudioState>["setState"],
  get: StoreApi<StoryStudioState>["getState"],
  port: StoryStudioPort,
  scope: { ownerUserId: string; presentationId: string },
  segmentId: string,
  outcome: Awaited<ReturnType<StoryStudioPort["startVideo"]>>,
) {
  set((state) => ({
    projects: replaceProject(state.projects, outcome.project),
    videoProgress: outcome.terminal
      ? withoutKey(state.videoProgress, segmentId)
      : { ...state.videoProgress, [segmentId]: { status: outcome.status, percent: outcome.percent } },
  }));
  if (outcome.terminal) return;
  const key = `${scope.presentationId}:${outcome.project.id}:${segmentId}`;
  const timer = window.setTimeout(async () => {
    pollTimers.delete(key);
    if (!sameScope(get(), scope)) return;
    try {
      const next = await port.pollVideo({ ...scope, projectId: outcome.project.id, segmentId });
      if (!sameScope(get(), scope)) return;
      acceptVideoOutcome(set, get, port, scope, segmentId, next);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        error: errorMessage(error, "剧情视频状态查询失败。"),
        videoProgress: withoutKey(state.videoProgress, segmentId),
      }));
    }
  }, 10_000);
  pollTimers.set(key, timer);
}

function installPlanningWatch(
  port: StoryStudioPort,
  scope: { ownerUserId: string; presentationId: string },
  activation: number,
  set: StoreApi<StoryStudioState>["setState"],
  get: StoreApi<StoryStudioState>["getState"],
) {
  const watch = ++planningWatchSequence;
  void port.watchPlanning(scope, (event) => {
    if (watch !== planningWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) return;
    const progress = planningProgress(event);
    set((state) => {
      const current = state.planningProgress[progress.projectId];
      if (current?.runId && current.updatedAtUnixMs > progress.updatedAtUnixMs) return state;
      return {
        planningProgress: { ...state.planningProgress, [progress.projectId]: progress },
      };
    });
  }).then((unlisten) => {
    if (watch !== planningWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) {
      unlisten();
      return;
    }
    planningUnlisten = unlisten;
  }).catch((error: unknown) => {
    if (watch !== planningWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) return;
    set({ error: errorMessage(error, "剧情规划进度暂时无法接收。") });
  });
}

function planningProgress(event: StoryPlanningEvent): StoryPlanningProgress {
  return planningProgressFromRun(event.run);
}

function planningProgressFromRun(run: StoryPlanningRun): StoryPlanningProgress {
  return {
    runId: run.id,
    projectId: run.projectId,
    stage: run.stage,
    status: run.status,
    stopReason: run.stopReason,
    modelCalls: run.modelCalls,
    updatedAtUnixMs: run.updatedAtUnixMs,
  };
}

function stopPlanningWatch() {
  planningWatchSequence += 1;
  planningUnlisten?.();
  planningUnlisten = null;
}

function installMediaBatchWatch(
  port: StoryStudioPort,
  scope: { ownerUserId: string; presentationId: string },
  activation: number,
  set: StoreApi<StoryStudioState>["setState"],
  get: StoreApi<StoryStudioState>["getState"],
) {
  const watch = ++mediaBatchWatchSequence;
  void port.watchMediaBatches(scope, (event: StoryMediaBatchEvent) => {
    if (watch !== mediaBatchWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) return;
    set((state) => ({
      projects: replaceProject(state.projects, event.project),
      mediaBatches: {
        ...state.mediaBatches,
        [event.batch.projectId]: replaceMediaBatch(
          state.mediaBatches[event.batch.projectId] ?? [],
          event.batch,
        ),
      },
    }));
  }).then((unlisten) => {
    if (watch !== mediaBatchWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) {
      unlisten();
      return;
    }
    mediaBatchUnlisten = unlisten;
  }).catch((error: unknown) => {
    if (watch !== mediaBatchWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) return;
    set({ error: errorMessage(error, "批量制作进度暂时无法接收。") });
  });
}

function stopMediaBatchWatch() {
  mediaBatchWatchSequence += 1;
  mediaBatchUnlisten?.();
  mediaBatchUnlisten = null;
}

function installVideoGenerationWatch(
  port: StoryStudioPort,
  scope: { ownerUserId: string; presentationId: string },
  activation: number,
  set: StoreApi<StoryStudioState>["setState"],
  get: StoreApi<StoryStudioState>["getState"],
) {
  const watch = ++videoGenerationWatchSequence;
  void port.watchVideoGeneration(scope, (event: StoryVideoGenerationEvent) => {
    if (watch !== videoGenerationWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) return;
    set((state) => ({ projects: replaceProject(state.projects, event.project) }));
  }).then((unlisten) => {
    if (watch !== videoGenerationWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) {
      unlisten();
      return;
    }
    videoGenerationUnlisten = unlisten;
  }).catch((error: unknown) => {
    if (watch !== videoGenerationWatchSequence
      || !isActive(get(), activation, scope.ownerUserId, scope.presentationId)) return;
    set({ error: errorMessage(error, "剧情视频进度暂时无法接收。") });
  });
}

function stopVideoGenerationWatch() {
  videoGenerationWatchSequence += 1;
  videoGenerationUnlisten?.();
  videoGenerationUnlisten = null;
}

function selectedProject(state: StoryStudioState) {
  return state.projects.find((project) => project.id === state.selectedProjectId) ?? null;
}

function activeScope(state: StoryStudioState) {
  return state.ownerUserId && state.presentationId
    ? { ownerUserId: state.ownerUserId, presentationId: state.presentationId }
    : null;
}

function sameScope(state: StoryStudioState, scope: { ownerUserId: string; presentationId: string }) {
  return state.ownerUserId === scope.ownerUserId && state.presentationId === scope.presentationId;
}

function isActive(
  state: StoryStudioState,
  sequence: number,
  ownerUserId: string,
  presentationId: string,
) {
  return sequence === activationSequence && sameScope(state, { ownerUserId, presentationId });
}

function replaceProject(projects: StoryProject[], project: StoryProject) {
  return projects.map((value) => value.id === project.id ? project : value);
}

function replacePlanningRun(runs: StoryPlanningRun[], run: StoryPlanningRun) {
  return [run, ...runs.filter((value) => value.id !== run.id)]
    .sort((left, right) => right.updatedAtUnixMs - left.updatedAtUnixMs);
}

function mergePlanningRuns(current: StoryPlanningRun[], incoming: StoryPlanningRun[]) {
  return incoming.reduce(replacePlanningRun, current);
}

function replaceMediaBatch(batches: StoryMediaBatchSnapshot[], batch: StoryMediaBatchSnapshot) {
  return [batch, ...batches.filter((value) => value.id !== batch.id)]
    .sort((left, right) => right.updatedAtUnixMs - left.updatedAtUnixMs);
}

function mergeMediaBatches(
  current: StoryMediaBatchSnapshot[],
  incoming: StoryMediaBatchSnapshot[],
) {
  return incoming.reduce(replaceMediaBatch, current);
}

function withoutKey<T>(values: Record<string, T>, key: string) {
  const next = { ...values };
  delete next[key];
  return next;
}

function clearPollTimers() {
  for (const timer of pollTimers.values()) window.clearTimeout(timer);
  pollTimers.clear();
}

function errorMessage(error: unknown, fallback: string) {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return fallback;
}

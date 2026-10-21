import type {
  StoryProject,
  StoryProjectCursor,
  StoryProjectPage,
  StoryPlanningEvent,
  StoryPlanningHistoryPage,
  StoryPlanningRun,
  StoryMediaBatchKind,
  StoryMediaBatchPreviewResult,
  StoryMediaBatchHistoryPage,
  StoryMediaBatchPreparation,
  StoryMediaBatchSnapshot,
  StoryVideoGenerationOutcome,
} from "../domain/storyStudio";

export interface StoryPresentationScope {
  ownerUserId: string;
  presentationId: string;
}

export type StoryImageImportTarget =
  | { kind: "resource"; resourceId: string }
  | { kind: "frame"; segmentId: string; frameRole: "first" | "last" };

export interface StoryStudioPort {
  watchPlanning(
    request: StoryPresentationScope,
    listener: (event: StoryPlanningEvent) => void,
  ): Promise<() => void>;
  loadProjects(request: StoryPresentationScope & {
    after?: StoryProjectCursor | null;
    limit?: number | null;
  }): Promise<StoryProjectPage>;
  loadProject(request: StoryPresentationScope & { projectId: string }): Promise<StoryProject>;
  previewMediaBatch(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    kind: StoryMediaBatchKind;
    targetIds?: string[] | null;
    userIdeas: string;
  }): Promise<StoryMediaBatchPreviewResult>;
  loadMediaBatchHistory(request: StoryPresentationScope & {
    projectId: string;
    after?: StoryProjectCursor | null;
    limit?: number | null;
  }): Promise<StoryMediaBatchHistoryPage>;
  startMediaBatch(request: StoryPresentationScope & {
    preparation: StoryMediaBatchPreparation;
    userIdeas: string;
  }): Promise<StoryMediaBatchSnapshot>;
  abandonMediaBatch(request: StoryPresentationScope & {
    projectId: string;
    batchId: string;
  }): Promise<StoryMediaBatchSnapshot>;
  createProject(request: StoryPresentationScope & {
    title: string;
    description: string;
    textModelId: string;
    imageModelId: string;
    videoModelId: string;
  }): Promise<StoryProject>;
  updateSource(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    source: string;
    style: string;
    ratio: string;
  }): Promise<StoryProject>;
  startPlanning(request: StoryPresentationScope & {
    projectId: string;
    stage: "outline" | "refine";
    targetIds: string[];
    userIdeas: string;
  }): Promise<StoryProject | null>;
  loadPlanningHistory(request: StoryPresentationScope & {
    projectId: string;
    after?: StoryProjectCursor | null;
    limit?: number | null;
  }): Promise<StoryPlanningHistoryPage>;
  resumePlanning(request: StoryPresentationScope & {
    projectId: string;
    runId: string;
  }): Promise<StoryProject | null>;
  abandonPlanning(request: StoryPresentationScope & {
    projectId: string;
    runId: string;
  }): Promise<StoryPlanningRun>;
  importLocalImage(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    target: StoryImageImportTarget;
  }): Promise<StoryProject | null>;
  importHistoryImage(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    target: StoryImageImportTarget;
    recordId: string;
    contentToken: string;
  }): Promise<StoryProject>;
  updateSegmentDuration(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    seconds: number;
  }): Promise<StoryProject>;
  allowImageRetry(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    target: StoryImageImportTarget;
  }): Promise<StoryProject>;
  generateAssetImage(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    resourceId: string;
    userIdeas: string;
  }): Promise<StoryProject>;
  generateFrameImage(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    frameRole: "first" | "last";
    referenceResourceIds: string[];
    userIdeas: string;
  }): Promise<StoryProject>;
  confirmLatestResourceImages(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    resourceIds: string[];
  }): Promise<StoryProject>;
  confirmLatestFrames(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    useConfirmedLastFrameForVideo: boolean;
  }): Promise<StoryProject>;
  startVideo(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    size: "768P" | "2K";
    userIdeas: string;
  }): Promise<StoryVideoGenerationOutcome>;
  pollVideo(request: StoryPresentationScope & {
    projectId: string;
    segmentId: string;
  }): Promise<StoryVideoGenerationOutcome>;
  updateVideoGuidance(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    mode: "first_frame" | "first_and_last_frames" | "previous_video" | "source_video";
  }): Promise<StoryProject>;
  archiveCompletedVideo(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    useSourceVideo: boolean;
  }): Promise<StoryProject>;
  allowVideoRetry(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
  }): Promise<StoryProject>;
  discardPresentation(request: StoryPresentationScope): Promise<void>;
}
